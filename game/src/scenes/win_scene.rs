/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use crate::game_context::SharedGameContext;
use crate::scenes::title_scene::TitleSceneLogic;
use engine::assets::font::Font;
use engine::assets::texture::Texture;
use engine::context::UpdateCtx;
use engine::game_director::{GameSceneLogic, SceneTransition};
use engine::renderer::BASE_VIRTUAL_WIDTH;
use engine::scene::{
    LocalTransform, Material, Scene, SpriteComponent, TextAnchor, TextComponent, TextLayout,
    TextStyle,
};
use glam::Vec2;

pub struct WinSceneLogic {
    game_context: SharedGameContext,
}

impl WinSceneLogic {
    pub fn new(game_context: SharedGameContext) -> Self {
        Self { game_context }
    }
}

impl GameSceneLogic for WinSceneLogic {
    fn on_enter(&mut self, scene: &mut Scene, ctx: &mut UpdateCtx<'_>) {
        let game_ctx = self.game_context.borrow();
        let jersey_font = ctx.assets.get_handle::<Font>("Tiny5Duo").unwrap();
        let tiny_font = ctx.assets.get_handle::<Font>("Tiny5").unwrap();
        let qr_tex = ctx
            .assets
            .get_handle::<Texture>("assets/textures/qrcode.png")
            .unwrap();
        let qr_mat = Material::new_opaque(qr_tex);
        let logo_tex = ctx
            .assets
            .get_handle::<Texture>("assets/textures/disorderly.png")
            .unwrap();
        let logo_mat = Material::new_opaque(logo_tex);

        scene.spawn_text_entity(
            LocalTransform::from_xyz_screenspace(0.0, -70.0, 10.0),
            TextComponent::new(
                game_ctx.locales.get_string("win.title"),
                TextStyle {
                    font: jersey_font,
                    line_height: None,
                    scale: 2.0,
                    color: [245, 245, 245, 255],
                },
                TextLayout {
                    max_width: Some(BASE_VIRTUAL_WIDTH as f32),
                    max_height: None,
                    anchor: TextAnchor::HorizonCenter,
                },
            ),
        );
        scene.spawn_text_entity(
            LocalTransform::from_xyz_screenspace(-10.0, 0.0, 10.0),
            TextComponent::new(
                game_ctx.locales.get_string("win.text"),
                TextStyle {
                    font: tiny_font,
                    line_height: None,
                    scale: 1.0,
                    color: [255, 255, 255, 255],
                },
                TextLayout {
                    max_width: Some(BASE_VIRTUAL_WIDTH as f32 * 0.4),
                    max_height: None,
                    anchor: TextAnchor::HorizonRight,
                },
            ),
        );
        scene.spawn_sprite_entity(
            LocalTransform::from_xyz_screenspace(70.0, 0.0, 10.0),
            SpriteComponent::new(qr_mat, Vec2::new(2.0 * 39.0, 2.0 * 39.0)),
        );
        scene.spawn_sprite_entity(
            LocalTransform::from_xyz_screenspace(0.0, 70.0, 10.0),
            SpriteComponent::new(logo_mat, Vec2::new(40.0, 40.0)),
        );
    }

    fn update(
        &mut self,
        _scene: &mut Scene,
        _dt: std::time::Duration,
        ctx: &mut UpdateCtx<'_>,
    ) -> SceneTransition {
        if ctx.inputs.is_action_just_pressed("menu_select") {
            SceneTransition::Replace(Box::new(TitleSceneLogic::new(self.game_context.clone())))
        } else {
            SceneTransition::None
        }
    }
}
