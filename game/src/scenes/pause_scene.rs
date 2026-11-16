/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::game_context::SharedGameContext;
use crate::scenes::title_scene::TitleSceneLogic;
use engine::assets::font::Font;
use engine::assets::texture::Texture;
use engine::context::UpdateCtx;
use engine::game_director::{GameSceneLogic, SceneTransition};
use engine::renderer::{BASE_VIRTUAL_HEIGHT, BASE_VIRTUAL_WIDTH};
use engine::scene::{
    LocalTransform, Material, Scene, SceneFlags, SpriteComponent, TextAnchor, TextComponent,
    TextLayout, TextStyle,
};
use glam::{Vec2, Vec3};

const WHITE: [u8; 4] = [255, 255, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];
const VOLUME_STEP: u8 = 10;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum MenuSelection {
    Resume,
    MasterVolume,
    SfxVolume,
    MusicVolume,
    Rumble,
    Language,
    ExitGame,
}

impl MenuSelection {
    fn next(self) -> Self {
        match self {
            Self::Resume => Self::MasterVolume,
            Self::MasterVolume => Self::SfxVolume,
            Self::SfxVolume => Self::MusicVolume,
            Self::MusicVolume => Self::Rumble,
            Self::Rumble => Self::Language,
            Self::Language => Self::ExitGame,
            Self::ExitGame => Self::Resume,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Resume => Self::ExitGame,
            Self::MasterVolume => Self::Resume,
            Self::SfxVolume => Self::MasterVolume,
            Self::MusicVolume => Self::SfxVolume,
            Self::Rumble => Self::MusicVolume,
            Self::Language => Self::Rumble,
            Self::ExitGame => Self::Language,
        }
    }
}

pub struct PauseSceneLogic {
    from_title: bool,
    selection: MenuSelection,
    title_text: Option<engine::scene::EntityHandle>,
    resume_text: Option<engine::scene::EntityHandle>,
    master_label: Option<engine::scene::EntityHandle>,
    master_value: Option<engine::scene::EntityHandle>,
    sfx_label: Option<engine::scene::EntityHandle>,
    sfx_value: Option<engine::scene::EntityHandle>,
    music_label: Option<engine::scene::EntityHandle>,
    music_value: Option<engine::scene::EntityHandle>,
    rumble_label: Option<engine::scene::EntityHandle>,
    rumble_value: Option<engine::scene::EntityHandle>,
    language_label: Option<engine::scene::EntityHandle>,
    language_value: Option<engine::scene::EntityHandle>,
    language_attribution: Option<engine::scene::EntityHandle>,
    exit_text: Option<engine::scene::EntityHandle>,
    overlay: Option<engine::scene::EntityHandle>,
    game_context: SharedGameContext,
}

impl PauseSceneLogic {
    pub fn new(game_context: SharedGameContext, from_title: bool) -> Self {
        Self {
            from_title,
            selection: MenuSelection::Resume,
            title_text: None,
            resume_text: None,
            master_label: None,
            master_value: None,
            sfx_label: None,
            sfx_value: None,
            music_label: None,
            music_value: None,
            rumble_label: None,
            rumble_value: None,
            language_label: None,
            language_value: None,
            language_attribution: None,
            exit_text: None,
            overlay: None,
            game_context,
        }
    }

    fn title_style(font: engine::assets::AssetHandle<Font>) -> TextStyle {
        TextStyle {
            font,
            line_height: None,
            scale: 2.0,
            color: WHITE,
        }
    }

    fn menu_style(font: engine::assets::AssetHandle<Font>, selected: bool) -> TextStyle {
        TextStyle {
            font,
            line_height: None,
            scale: 1.0,
            color: if selected { YELLOW } else { WHITE },
        }
    }

    fn spawn_text(
        scene: &mut Scene,
        x: f32,
        y: f32,
        z: f32,
        text: impl Into<String>,
        style: TextStyle,
        layout: TextLayout,
    ) -> engine::scene::EntityHandle {
        scene.spawn_text_entity(
            LocalTransform::from_xyz_screenspace(x, y, z),
            TextComponent::new(text.into(), style, layout),
        )
    }

    fn menu_layout() -> TextLayout {
        TextLayout {
            max_width: Some(BASE_VIRTUAL_WIDTH as f32),
            max_height: None,
            anchor: TextAnchor::HorizonCenter,
        }
    }

    fn set_text_color(
        scene: &mut Scene,
        entity: Option<engine::scene::EntityHandle>,
        color: [u8; 4],
    ) {
        let Some(entity) = entity else {
            return;
        };
        if let Some(text) = scene.get_component_mut::<TextComponent>(entity) {
            text.update_style(|style| style.color = color);
        }
    }

    fn set_selected_colors(&self, scene: &mut Scene) {
        Self::set_text_color(
            scene,
            self.resume_text,
            if self.selection == MenuSelection::Resume {
                YELLOW
            } else {
                WHITE
            },
        );
        let master_selected = self.selection == MenuSelection::MasterVolume;
        Self::set_text_color(
            scene,
            self.master_label,
            if master_selected { YELLOW } else { WHITE },
        );
        Self::set_text_color(
            scene,
            self.master_value,
            if master_selected { YELLOW } else { WHITE },
        );
        let sfx_selected = self.selection == MenuSelection::SfxVolume;
        Self::set_text_color(
            scene,
            self.sfx_label,
            if sfx_selected { YELLOW } else { WHITE },
        );
        Self::set_text_color(
            scene,
            self.sfx_value,
            if sfx_selected { YELLOW } else { WHITE },
        );
        let music_selected = self.selection == MenuSelection::MusicVolume;
        Self::set_text_color(
            scene,
            self.music_label,
            if music_selected { YELLOW } else { WHITE },
        );
        Self::set_text_color(
            scene,
            self.music_value,
            if music_selected { YELLOW } else { WHITE },
        );
        let rumble_selected = self.selection == MenuSelection::Rumble;
        Self::set_text_color(
            scene,
            self.rumble_label,
            if rumble_selected { YELLOW } else { WHITE },
        );
        Self::set_text_color(
            scene,
            self.rumble_value,
            if rumble_selected { YELLOW } else { WHITE },
        );
        let language_selected = self.selection == MenuSelection::Language;
        Self::set_text_color(
            scene,
            self.language_label,
            if language_selected { YELLOW } else { WHITE },
        );
        Self::set_text_color(
            scene,
            self.language_value,
            if language_selected { YELLOW } else { WHITE },
        );
        Self::set_text_color(
            scene,
            self.exit_text,
            if self.selection == MenuSelection::ExitGame {
                YELLOW
            } else {
                WHITE
            },
        );
    }

    fn set_text(scene: &mut Scene, entity: Option<engine::scene::EntityHandle>, text: String) {
        let Some(entity) = entity else {
            return;
        };
        if let Some(component) = scene.get_component_mut::<TextComponent>(entity) {
            component.set_text(text);
        }
    }

    fn update_localized_labels(&self, scene: &mut Scene, ctx: &UpdateCtx<'_>) {
        let Ok(context) = self.game_context.try_borrow() else {
            return;
        };

        Self::set_text(
            scene,
            self.title_text,
            if self.from_title {
                context.locales.get_string("pause.options")
            } else {
                context.locales.get_string("pause.pause")
            },
        );
        Self::set_text(
            scene,
            self.resume_text,
            context.locales.get_string("pause.resume"),
        );
        Self::set_text(
            scene,
            self.master_label,
            context.locales.get_string("pause.volume.master"),
        );
        Self::set_text(
            scene,
            self.sfx_label,
            context.locales.get_string("pause.volume.sfx"),
        );
        Self::set_text(
            scene,
            self.music_label,
            context.locales.get_string("pause.volume.music"),
        );
        Self::set_text(
            scene,
            self.rumble_label,
            context.locales.get_string("pause.rumble"),
        );
        Self::set_text(
            scene,
            self.rumble_value,
            if ctx.inputs.is_rumble_enabled() {
                context.locales.get_string("pause.value_true")
            } else {
                context.locales.get_string("pause.value_false")
            },
        );
        Self::set_text(
            scene,
            self.language_label,
            context.locales.get_string("pause.language"),
        );

        let current_lang_label = context
            .locales
            .get_language(context.config.lang_id())
            .map(|lang| lang.language_name().to_string())
            .unwrap_or_else(|| context.config.lang_id().to_string());
        Self::set_text(scene, self.language_value, current_lang_label);

        Self::set_text(
            scene,
            self.language_attribution,
            context.locales.get_string("_attribution"),
        );

        Self::set_text(
            scene,
            self.exit_text,
            context.locales.get_string("pause.exit"),
        );
    }

    fn update_volume_labels(&self, scene: &mut Scene, ctx: &UpdateCtx<'_>) {
        if let Some(text) = self
            .master_value
            .and_then(|entity| scene.get_component_mut::<TextComponent>(entity))
        {
            text.set_text(format!("{}%", ctx.audio.master_volume_percent()));
        }
        if let Some(text) = self
            .sfx_value
            .and_then(|entity| scene.get_component_mut::<TextComponent>(entity))
        {
            text.set_text(format!("{}%", ctx.audio.sfx_volume_percent()));
        }
        if let Some(text) = self
            .music_value
            .and_then(|entity| scene.get_component_mut::<TextComponent>(entity))
        {
            text.set_text(format!("{}%", ctx.audio.music_volume_percent()));
        }
        self.update_localized_labels(scene, ctx);
    }

    fn adjust_volume(percent: u8, delta: i8) -> u8 {
        let next = i16::from(percent) + i16::from(delta) * i16::from(VOLUME_STEP);
        next.clamp(0, 100) as u8
    }

    fn sync_config_to_state(&self, ctx: &mut UpdateCtx<'_>) {
        if let Ok(mut context) = self.game_context.try_borrow_mut() {
            context.set_master_volume_percent(ctx.audio.master_volume_percent());
            context.set_sfx_volume_percent(ctx.audio.sfx_volume_percent());
            context.set_music_volume_percent(ctx.audio.music_volume_percent());
            context.set_rumble_enabled(ctx.inputs.is_rumble_enabled());
            context.apply_to_audio_input(ctx.audio, ctx.inputs);
        }
    }

    fn switch_language(&self, delta: i8, scene: &mut Scene, ctx: &mut UpdateCtx<'_>) {
        if let Ok(mut context) = self.game_context.try_borrow_mut() {
            let lang_ids: Vec<String> = context
                .locales
                .languages()
                .iter()
                .map(|lang| lang.id().to_string())
                .collect();
            if lang_ids.is_empty() {
                return;
            }

            let current_idx = lang_ids
                .iter()
                .position(|id| id == context.config.lang_id())
                .unwrap_or(0);
            let next_idx =
                (current_idx as i32 + delta as i32).rem_euclid(lang_ids.len() as i32) as usize;
            let next_lang_id = &lang_ids[next_idx];

            if context.locales.load_language(next_lang_id) {
                context.config.set_lang_id(next_lang_id);
                context.config.ensure_saved();
            }
        }

        self.update_localized_labels(scene, ctx);
    }
}

impl GameSceneLogic for PauseSceneLogic {
    fn on_enter(&mut self, scene: &mut Scene, ctx: &mut UpdateCtx<'_>) {
        {
            let context = self.game_context.borrow();
            context.apply_to_audio_input(&mut ctx.audio, &mut ctx.inputs);
        }
        if self.from_title {
            scene.set_scene_flags(SceneFlags::DRAW_BELOW | SceneFlags::UPDATE_BELOW);
        } else {
            scene.set_scene_flags(SceneFlags::DRAW_BELOW);
        }

        let overlay_tex = ctx
            .assets
            .get_handle::<Texture>("assets/textures/pause_overlay.png")
            .unwrap();
        let overlay_mat = Material::new_alpha(overlay_tex);
        let mut overlay_transform = LocalTransform::from_xyz_screenspace(0.0, 0.0, -10.0);
        overlay_transform.scale = Vec2::new(
            BASE_VIRTUAL_WIDTH as f32 * 3.0,
            BASE_VIRTUAL_HEIGHT as f32 * 3.0,
        );
        overlay_transform.position = Vec3::new(0.0, 0.0, -10.0);
        self.overlay = Some(scene.spawn_sprite_entity(
            overlay_transform,
            SpriteComponent::new(overlay_mat, Vec2::new(1.0, 1.0)),
        ));

        let title_font = ctx.assets.get_handle::<Font>("Tiny5Duo").unwrap();
        let small_font = ctx.assets.get_handle::<Font>("Tiny5").unwrap();

        self.title_text = Some(Self::spawn_text(
            scene,
            0.0,
            -78.0,
            10.0,
            "PAUSE",
            Self::title_style(title_font),
            Self::menu_layout(),
        ));

        self.resume_text = Some(Self::spawn_text(
            scene,
            0.0,
            -18.0,
            10.0,
            "Resume",
            Self::menu_style(small_font, true),
            Self::menu_layout(),
        ));

        self.master_label = Some(Self::spawn_text(
            scene,
            -46.0,
            12.0,
            10.0,
            "Master volume",
            Self::menu_style(small_font, false),
            Self::menu_layout(),
        ));
        self.master_value = Some(Self::spawn_text(
            scene,
            52.0,
            12.0,
            10.0,
            format!("{}%", ctx.audio.master_volume_percent()),
            Self::menu_style(small_font, false),
            Self::menu_layout(),
        ));

        self.sfx_label = Some(Self::spawn_text(
            scene,
            -46.0,
            22.0,
            10.0,
            "SFX volume",
            Self::menu_style(small_font, false),
            Self::menu_layout(),
        ));
        self.sfx_value = Some(Self::spawn_text(
            scene,
            52.0,
            22.0,
            10.0,
            format!("{}%", ctx.audio.sfx_volume_percent()),
            Self::menu_style(small_font, false),
            Self::menu_layout(),
        ));

        self.music_label = Some(Self::spawn_text(
            scene,
            -46.0,
            32.0,
            10.0,
            "Music volume",
            Self::menu_style(small_font, false),
            Self::menu_layout(),
        ));
        self.music_value = Some(Self::spawn_text(
            scene,
            52.0,
            32.0,
            10.0,
            format!("{}%", ctx.audio.music_volume_percent()),
            Self::menu_style(small_font, false),
            Self::menu_layout(),
        ));

        self.rumble_label = Some(Self::spawn_text(
            scene,
            -46.0,
            42.0,
            10.0,
            "Gamepad rumble",
            Self::menu_style(small_font, false),
            Self::menu_layout(),
        ));

        self.rumble_value = Some(Self::spawn_text(
            scene,
            52.0,
            42.0,
            10.0,
            "ON",
            Self::menu_style(small_font, false),
            Self::menu_layout(),
        ));

        self.language_label = Some(Self::spawn_text(
            scene,
            -46.0,
            52.0,
            10.0,
            "Language",
            Self::menu_style(small_font, false),
            Self::menu_layout(),
        ));

        self.language_value = Some(Self::spawn_text(
            scene,
            52.0,
            52.0,
            10.0,
            "English",
            Self::menu_style(small_font, false),
            Self::menu_layout(),
        ));

        self.language_attribution = Some(Self::spawn_text(
            scene,
            0.0,
            62.0,
            10.0,
            "",
            TextStyle {
                font: small_font,
                line_height: None,
                scale: 1.0,
                color: [192, 192, 192, 255],
            },
            Self::menu_layout(),
        ));

        self.exit_text = Some(Self::spawn_text(
            scene,
            0.0,
            82.0,
            10.0,
            "Exit game",
            Self::menu_style(small_font, false),
            Self::menu_layout(),
        ));

        self.set_selected_colors(scene);
        self.update_volume_labels(scene, ctx);
    }

    fn on_exit(&mut self, _scene: &mut Scene, _ctx: &mut UpdateCtx<'_>) {
        self.overlay = None;
        if let Ok(mut context) = self.game_context.try_borrow_mut() {
            context.ensure_config_saved();
        }
    }

    fn update(
        &mut self,
        scene: &mut Scene,
        _dt: std::time::Duration,
        ctx: &mut UpdateCtx<'_>,
    ) -> SceneTransition {
        if ctx.inputs.is_action_just_pressed("menu_pause")
            || ctx.inputs.is_action_just_pressed("cancel")
        {
            return SceneTransition::Pop;
        }

        if ctx.inputs.is_action_just_pressed("menu_select") {
            match self.selection {
                MenuSelection::Resume => return SceneTransition::Pop,
                MenuSelection::Rumble => {
                    ctx.inputs
                        .set_rumble_enabled(!ctx.inputs.is_rumble_enabled());
                    self.update_volume_labels(scene, ctx);
                    self.sync_config_to_state(ctx);
                }
                MenuSelection::ExitGame => {
                    self.sync_config_to_state(ctx);
                    if let Ok(mut context) = self.game_context.try_borrow_mut() {
                        context.ensure_config_saved();
                    }
                    if self.from_title {
                        std::process::exit(0);
                    } else {
                        return SceneTransition::PopAndReplace(Box::new(TitleSceneLogic::new(
                            self.game_context.clone(),
                        )));
                    }
                }
                _ => {}
            }
        }

        if ctx.inputs.is_action_just_pressed("menu_up") {
            self.selection = self.selection.prev();
            self.set_selected_colors(scene);
        } else if ctx.inputs.is_action_just_pressed("menu_down") {
            self.selection = self.selection.next();
            self.set_selected_colors(scene);
        }

        let change = if ctx.inputs.is_action_just_pressed("menu_left") {
            Some(-1)
        } else if ctx.inputs.is_action_just_pressed("menu_right") {
            Some(1)
        } else {
            None
        };

        if let Some(change) = change {
            match self.selection {
                MenuSelection::MasterVolume => {
                    let current = ctx.audio.master_volume_percent();
                    ctx.audio
                        .set_master_volume_percent(Self::adjust_volume(current, change));
                    self.update_volume_labels(scene, ctx);
                    self.sync_config_to_state(ctx);
                }
                MenuSelection::SfxVolume => {
                    let current = ctx.audio.sfx_volume_percent();
                    ctx.audio
                        .set_sfx_volume_percent(Self::adjust_volume(current, change));
                    self.update_volume_labels(scene, ctx);
                    self.sync_config_to_state(ctx);
                }
                MenuSelection::MusicVolume => {
                    let current = ctx.audio.music_volume_percent();
                    ctx.audio
                        .set_music_volume_percent(Self::adjust_volume(current, change));
                    self.update_volume_labels(scene, ctx);
                    self.sync_config_to_state(ctx);
                }
                MenuSelection::Rumble => {
                    ctx.inputs
                        .set_rumble_enabled(!ctx.inputs.is_rumble_enabled());
                    self.update_volume_labels(scene, ctx);
                    self.sync_config_to_state(ctx);
                }
                MenuSelection::Language => {
                    self.switch_language(change, scene, ctx);
                }
                _ => {}
            }
        }

        SceneTransition::None
    }
}
