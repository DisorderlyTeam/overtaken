/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::entities::enemy::{EnemyComponent, EnemyState, EnemyTypes, PatrolStep};
use crate::entities::interactable::{
    InteractableComponent, InteractionAction, InteractionRequirements, OvertakingRequirement,
};
use crate::entities::player::PlayerComponent;
use crate::game_context::SharedGameContext;
use crate::level::{
    CheckpointObjectDef, EnemyObjectDef, Level, LevelData, LevelObjectDef, ObstacleObjectDef,
    PatrolStepDef, TriggerActionDef, TriggerNamedActionDef, TriggerObjectDef,
    VentEntranceObjectDef, VentLayer,
};
use crate::managers::locale_manager::{LocaleManager, format_timer};
use crate::scenes::level_end_scene::LevelEndSceneLogic;
use crate::scenes::pause_scene::PauseSceneLogic;
use crate::scenes::win_scene::WinSceneLogic;
use crate::time::GameTime;
use engine::assets::AssetHandle;
use engine::assets::AssetManager;
use engine::assets::audio::Audio;
use engine::assets::font::Font;
use engine::assets::texture::Texture;
use engine::context::UpdateCtx;
use engine::core::color::Color;
use engine::core::vec2d::Array2D;
use engine::game_director::{GameSceneLogic, SceneTransition};
use engine::input::{ControlType, GamepadType, InputManager, KeyboardKey, RumbleMotor};
use engine::renderer::{BASE_VIRTUAL_HEIGHT, BASE_VIRTUAL_WIDTH};
use engine::scene::collisions::{
    Collider, CollisionShape, DynamicCollider, StaticCollider, TriggerCollider,
};
use engine::scene::{
    AnimatedSpriteComponent, AnimatedSpriteDefinition, ColliderComponent, Component,
    ComponentTypeFlags, DebugDrawComponent, DebugDrawable, DebugShape, EntityHandle,
    GlobalTransform, LightSourceComponent, LocalTransform, Material, NavigationComponent, Scene,
    SpriteComponent, TextAnchor, TextComponent, TextLayout, TextStyle, TilemapComponent,
};
use glam::{UVec2, Vec2, Vec3, Vec4};
use std::collections::HashMap;
use std::fs;
use std::time::Duration;
use tracing::error;

const COL_LAYER_WALLS: u32 = 1 << 0;
const COL_LAYER_PLAYER: u32 = 1 << 1;
const COL_LAYER_ENEMIES: u32 = 1 << 2;
const COL_LAYER_FLYING_IGNORED: u32 = 1 << 3;
const COL_LAYER_TRIGGERS: u32 = 1 << 4;
const COL_LAYER_VENTS: u32 = 1 << 5;
const GAME_COLLIDES_WITH_DEFAULT: u32 =
    COL_LAYER_PLAYER | COL_LAYER_ENEMIES | COL_LAYER_FLYING_IGNORED | COL_LAYER_TRIGGERS;
const GAME_DYNAMIC_COLLIDES_WITH_DEFAULT: u32 = COL_LAYER_WALLS | GAME_COLLIDES_WITH_DEFAULT;
const LIGHTING_LAYER_GROUND: u8 = 0;
const LIGHTING_LAYER_VENTS: u8 = 1;
const NAVIGATION_GROUND_NAME: &str = "navigation_ground";
const NAVIGATION_FLYING_NAME: &str = "navigation_flying";
const ENEMY_VIEWCONE_NAME: &str = "enemy_viewcone";
const INTERACT_PROMPT_NAME: &str = "interact_prompt";
pub(crate) const OVERTAKE_RANGE_PX: f32 = 30.0;
const OBSTACLE_INTERACT_RANGE_PX: f32 = 18.0;
const OVERTAKE_BAR_TEXTURE_EMPTY: &str = "assets/textures/overtake_bar_empty.png";
const OVERTAKE_BAR_TEXTURE_FULL: &str = "assets/textures/overtake_bar_full.png";
const OVERTAKE_BAR_W_PX: f32 = 112.0;
const OVERTAKE_BAR_H_PX: f32 = 16.0;
const PLAYER_KILL_CONTACT_PX: f32 = 10.0;
const PLAYER_DEATH_RESTART_DELAY_S: f32 = 3.0;
const CHECKPOINT_SIZE_PX: f32 = 16.0;
const TIME_SCALES: [f32; 3] = [1.0, 0.5, 0.25];
const ENEMY_TYPES_PATH: &str = "assets/data/enemy_types.json";
// Render Z-ordering (higher Z appears on top):
// Sky/background: -100
// Tilemap: 0
// Tilemap objects/enemies: 50
// Player (ground): 75
// Vents (tilemap): 100
// Vent entrances: 110
// Player (in vents): 125
// UI (overtake bar): 200
const Z_SKY: f32 = -100.0;
const Z_TILEMAP: f32 = 0.0;
const Z_OBJECTS_ENEMIES: f32 = 50.0;
const Z_PLAYER_GROUND: f32 = 75.0;
const Z_VENTS_TILEMAP: f32 = 100.0;
const Z_VENT_ENTRANCE: f32 = 110.0;
const Z_PLAYER_VENTS: f32 = 125.0;
const Z_UI: f32 = 200.0;

struct OvertakeSession {
    enemy: EntityHandle,
    remaining_s: f32,
    overtake_duration_s: f32,
    overtaken_speed: f32,
    player_collider: Option<ColliderComponent>,
    leave_sound: Option<AssetHandle<Audio>>,
}

struct PendingOvertake {
    enemy: EntityHandle,
    player_collider: Option<ColliderComponent>,
    possess_sound: Option<AssetHandle<Audio>>,
    leave_sound: Option<AssetHandle<Audio>>,
}

#[derive(Debug, Clone)]
struct VentEntranceComponent {
    open_material: Material,
    closed_material: Material,
    is_open: bool,
}

impl VentEntranceComponent {
    fn new(open_material: Material, closed_material: Material, start_open: bool) -> Self {
        Self {
            open_material,
            closed_material,
            is_open: start_open,
        }
    }

    fn is_open(&self) -> bool {
        self.is_open
    }

    fn current_material(&self) -> Material {
        if self.is_open {
            self.open_material
        } else {
            self.closed_material
        }
    }

    fn set_open(&mut self, open: bool) {
        self.is_open = open;
    }
}

impl Component for VentEntranceComponent {
    fn flags() -> ComponentTypeFlags {
        ComponentTypeFlags::empty()
    }
}

struct LevelTrigger {
    entity: EntityHandle,
    action: TriggerActionDef,
}

struct CheckpointComponent {
    inactive_material: Material,
    active_material: Material,
    center: Vec2,
    active: bool,
}

impl Component for CheckpointComponent {
    fn flags() -> ComponentTypeFlags {
        ComponentTypeFlags::empty()
    }
}

pub struct MapSceneLogic {
    level_id: String,
    level: Option<Level>,
    sky_entity: Option<EntityHandle>,
    player_entity: Option<EntityHandle>,
    player_light_entity: Option<EntityHandle>,
    player_spotlight_pivot_entity: Option<EntityHandle>,
    vent_tilemap_entity: Option<EntityHandle>,
    pending_overtake: Option<PendingOvertake>,
    overtake_bar_empty_entity: Option<EntityHandle>,
    overtake_bar_fill_entity: Option<EntityHandle>,
    overtake_session: Option<OvertakeSession>,
    player_death_remaining_s: Option<f32>,
    activated_checkpoint: Option<Vec2>,
    checkpoints: Vec<EntityHandle>,
    triggers: Vec<LevelTrigger>,
    sky_time_s: f32,
    game_context: SharedGameContext,
    player_in_vent: bool,
    player_collides_with_mask: u32,
    debug_label_entity: Option<EntityHandle>,
    debug_status_entity: Option<EntityHandle>,
    debug_rect_entity: Option<EntityHandle>,
    show_text_debug_overlay: bool,
    last_debug_hotkeys_handled_frame: Option<u64>,
    time_scale_index: usize,
    timer_entity: Option<EntityHandle>,
    elapsed: Duration,
}

impl MapSceneLogic {
    pub fn new(level: Level, level_id: impl Into<String>, game_context: SharedGameContext) -> Self {
        Self {
            level_id: level_id.into(),
            level: Some(level),
            sky_entity: None,
            player_entity: None,
            player_light_entity: None,
            player_spotlight_pivot_entity: None,
            vent_tilemap_entity: None,
            pending_overtake: None,
            overtake_bar_empty_entity: None,
            overtake_bar_fill_entity: None,
            overtake_session: None,
            player_death_remaining_s: None,
            activated_checkpoint: None,
            checkpoints: Vec::new(),
            triggers: Vec::new(),
            sky_time_s: 0.0,
            game_context,
            player_in_vent: false,
            player_collides_with_mask: u32::MAX,
            debug_label_entity: None,
            debug_status_entity: None,
            debug_rect_entity: None,
            show_text_debug_overlay: false,
            last_debug_hotkeys_handled_frame: None,
            time_scale_index: 0,
            timer_entity: None,
            elapsed: Duration::ZERO,
        }
    }

    fn with_activated_checkpoint(mut self, checkpoint: Option<Vec2>) -> Self {
        self.activated_checkpoint = checkpoint;
        self
    }

    fn with_elapsed(mut self, elapsed: Duration) -> Self {
        self.elapsed = elapsed;
        self
    }

    fn save_completion_data(&self) {
        let level_id = self.level_id.clone();
        let time = GameTime::from(self.elapsed);

        let mut game_ctx = self.game_context.borrow_mut();
        let mut saves = &mut game_ctx.saves;
        let data = saves.save_data_mut();
        let level_data = data.level_data_mut(level_id);
        level_data.completed = true;
        if let Some(previous_best) = level_data.best_time {
            if previous_best > time {
                level_data.best_time = Some(time);
            } // else the previous time is best
        } else {
            level_data.best_time = Some(time);
        }
        saves.save().expect("couldn't save game data");
    }

    fn transition_for_trigger(
        &self,
        trigger: &LevelTrigger,
        ctx: &mut UpdateCtx<'_>,
    ) -> Option<SceneTransition> {
        match &trigger.action {
            TriggerActionDef::Named(TriggerNamedActionDef::Win) => {
                self.save_completion_data();
                Some(SceneTransition::Replace(Box::new(LevelEndSceneLogic::new(
                    self.game_context.clone(),
                    self.level_id.clone(),
                    self.elapsed.into(),
                ))))
            }
        }
    }

    fn play_music(ctx: &mut UpdateCtx<'_>, path: &str) {
        let track = ctx.assets.get_handle::<Audio>(path).unwrap();
        let track = ctx.assets.resolve_handle::<Audio>(track).unwrap();
        ctx.audio.play_music(track);
    }

    fn play_sfx(ctx: &mut UpdateCtx<'_>, sound: AssetHandle<Audio>) {
        let sound = ctx.assets.resolve_handle::<Audio>(sound).unwrap();
        ctx.audio.play_sfx(sound);
    }

    fn ensure_debug_overlay_entities(&mut self, scene: &mut Scene, ctx: &mut UpdateCtx<'_>) {
        let font_handle = ctx.assets.get_handle::<Font>("Tiny5").unwrap();

        let needs_label = self.debug_label_entity.is_none_or(|entity| {
            !scene.is_entity_alive(entity) || scene.get_component::<TextComponent>(entity).is_none()
        });
        if needs_label {
            let text_style = TextStyle {
                font: font_handle,
                line_height: None,
                scale: 1.0,
                color: [255, 255, 255, 255],
            };
            let text_layout = TextLayout {
                max_width: Some(BASE_VIRTUAL_WIDTH as f32),
                max_height: None,
                anchor: TextAnchor::TopLeft,
            };
            let label = scene.spawn_text_entity(
                LocalTransform::from_xyz_screenspace(
                    -(BASE_VIRTUAL_WIDTH as f32) / 2.0,
                    -(BASE_VIRTUAL_HEIGHT as f32) / 2.0,
                    1.0,
                ),
                TextComponent::new("", text_style, text_layout),
            );
            self.debug_label_entity = Some(label);
        }

        let needs_debug_rect = self.debug_rect_entity.is_none_or(|entity| {
            !scene.is_entity_alive(entity)
                || scene.get_component::<DebugDrawComponent>(entity).is_none()
        });
        if needs_debug_rect {
            let mut debug_rect_transform = LocalTransform::from_xyz(0.0, 0.0, 999.0);
            debug_rect_transform.screen_space = true;
            let debug_rect = scene.spawn_spatial_entity(debug_rect_transform);
            let _ = scene.add_component(
                debug_rect,
                DebugDrawComponent {
                    drawable: DebugDrawable {
                        shape: DebugShape::Rect {
                            width: BASE_VIRTUAL_WIDTH as f32,
                            height: BASE_VIRTUAL_HEIGHT as f32,
                        },
                        color: Vec4::new(1.0, 0.2, 0.2, 1.0),
                    },
                },
            );
            self.debug_rect_entity = Some(debug_rect);
        }

        let needs_debug_status = self.debug_status_entity.is_none_or(|entity| {
            !scene.is_entity_alive(entity) || scene.get_component::<TextComponent>(entity).is_none()
        });
        if needs_debug_status {
            let text_style = TextStyle {
                font: font_handle,
                line_height: None,
                scale: 1.0,
                color: [255, 64, 64, 255],
            };
            let text_layout = TextLayout {
                max_width: Some(BASE_VIRTUAL_WIDTH as f32),
                max_height: None,
                anchor: TextAnchor::BottomCenter,
            };
            let debug_status = scene.spawn_text_entity(
                LocalTransform::from_xyz_screenspace(
                    0.0,
                    (BASE_VIRTUAL_HEIGHT as f32) / 2.0 - 6.0,
                    2.0,
                ),
                TextComponent::new("", text_style, text_layout),
            );
            self.debug_status_entity = Some(debug_status);
        }
    }

    fn update_debug(&mut self, scene: &mut Scene, dt: Duration, ctx: &mut UpdateCtx<'_>) {
        if self.last_debug_hotkeys_handled_frame != Some(ctx.frame_index)
            && ctx.inputs.is_key_just_pressed(KeyboardKey::F12)
        {
            self.time_scale_index = (self.time_scale_index + 1) % TIME_SCALES.len();
            ctx.set_timescale(TIME_SCALES[self.time_scale_index]);
        }

        if self.last_debug_hotkeys_handled_frame != Some(ctx.frame_index) {
            self.last_debug_hotkeys_handled_frame = Some(ctx.frame_index);

            if ctx.inputs.is_key_just_pressed(KeyboardKey::Backtick) {
                let ctrl_pressed = ctx.inputs.is_key_pressed(KeyboardKey::LeftControl)
                    || ctx.inputs.is_key_pressed(KeyboardKey::RightControl);
                if ctrl_pressed {
                    tracing::info!(
                        "Ctrl + backtick pressed. Frame index: {}, dt: {:?}, timings: {:?}, renderer stats: {:?}, shown_debug_layers: {:#034b}",
                        ctx.frame_index,
                        dt,
                        ctx.last_frame_timings,
                        ctx.last_frame_stats,
                        *ctx.shown_debug_layers
                    );
                } else {
                    self.show_text_debug_overlay = !self.show_text_debug_overlay;
                    tracing::debug!(
                        "Backtick pressed, text debug overlay {}.",
                        if self.show_text_debug_overlay {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    );
                }
            }

            if ctx.inputs.is_key_just_pressed(KeyboardKey::F1) {
                *ctx.shown_debug_layers ^= 1 << 0;
            }
            if ctx.inputs.is_key_just_pressed(KeyboardKey::F2) {
                *ctx.shown_debug_layers ^= 1 << 1;
            }
            if ctx.inputs.is_key_just_pressed(KeyboardKey::F3) {
                *ctx.shown_debug_layers ^= 1 << 2;
            }
            if ctx.inputs.is_key_just_pressed(KeyboardKey::F4) {
                *ctx.shown_debug_layers ^= 1 << 3;
            }
            if ctx.inputs.is_key_just_pressed(KeyboardKey::F5) {
                *ctx.shown_debug_layers ^= 1 << 4;
            }
            if ctx.inputs.is_key_just_pressed(KeyboardKey::F6) {
                *ctx.shown_debug_layers ^= 1 << 5;
            }
            if ctx.inputs.is_key_just_pressed(KeyboardKey::F7) {
                *ctx.shown_debug_layers ^= 1 << 6;
            }
            if ctx.inputs.is_key_just_pressed(KeyboardKey::F8) {
                *ctx.shown_debug_layers ^= 1 << 7;
            }

            if ctx.inputs.is_key_just_pressed(KeyboardKey::F10) {
                *ctx.shown_debug_layers = if *ctx.shown_debug_layers == 0 {
                    u32::MAX
                } else {
                    0
                };
            }

            if cfg!(debug_assertions)
                && (ctx.inputs.is_key_pressed(KeyboardKey::LeftControl)
                    || ctx.inputs.is_key_pressed(KeyboardKey::RightControl))
                && let Some(player) = scene
                    .entities_with_component::<LocalTransform>()
                    .into_iter()
                    .find(|entity| scene.entity_name(*entity) == Some("player"))
            {
                if ctx.inputs.is_key_just_pressed(KeyboardKey::I)
                    && let Some(player_component) =
                        scene.get_component_mut::<crate::entities::player::PlayerComponent>(player)
                {
                    player_component.toggle_invincibility();
                    tracing::info!(
                        enabled = player_component.is_invincible(),
                        "Debug invincibility toggled"
                    );
                }

                if ctx.inputs.is_key_just_pressed(KeyboardKey::O)
                    && let Some(player_component) =
                        scene.get_component_mut::<crate::entities::player::PlayerComponent>(player)
                {
                    player_component.toggle_invisibility();
                    tracing::info!(
                        enabled = player_component.is_invisible(),
                        "Debug player invisibility toggled"
                    );
                }

                if ctx.inputs.is_key_just_pressed(KeyboardKey::P)
                    && let Some(player_component) =
                        scene.get_component_mut::<crate::entities::player::PlayerComponent>(player)
                {
                    player_component.toggle_enemy_pause();
                    tracing::info!(
                        enabled = player_component.are_enemies_paused(),
                        "Debug enemy pause toggled"
                    );
                }
            }
        }

        self.ensure_debug_overlay_entities(scene, ctx);

        if let Some(status_label) = self.debug_status_entity {
            let status_line = {
                let mut statuses = Vec::new();
                if cfg!(debug_assertions)
                    && let Some(player) = scene
                        .entities_with_component::<LocalTransform>()
                        .into_iter()
                        .find(|entity| scene.entity_name(*entity) == Some("player"))
                {
                    if scene
                        .get_component::<crate::entities::player::PlayerComponent>(player)
                        .is_some_and(crate::entities::player::PlayerComponent::is_invincible)
                    {
                        statuses.push("Invulnerable");
                    }
                    if scene
                        .get_component::<crate::entities::player::PlayerComponent>(player)
                        .is_some_and(crate::entities::player::PlayerComponent::is_invisible)
                    {
                        statuses.push("Invisible to enemies");
                    }
                    if scene
                        .get_component::<crate::entities::player::PlayerComponent>(player)
                        .is_some_and(crate::entities::player::PlayerComponent::are_enemies_paused)
                    {
                        statuses.push("Enemies paused");
                    }
                }
                statuses.join(" + ")
            };

            if let Some(status_text) = scene.get_component_mut::<TextComponent>(status_label) {
                status_text.set_text(status_line);
            }
        }

        if !self.show_text_debug_overlay
            && let Some(label) = self.debug_label_entity
            && let Some(text) = scene.get_component_mut::<TextComponent>(label)
            && !text.text().is_empty()
        {
            text.set_text("");
        }

        if self.show_text_debug_overlay && ctx.frame_index.is_multiple_of(10) {
            let player_xy = scene
                .entities_with_component::<LocalTransform>()
                .into_iter()
                .find(|entity| scene.entity_name(*entity) == Some("player"))
                .and_then(|player| {
                    scene
                        .get_component::<LocalTransform>(player)
                        .map(|transform| transform.position.truncate())
                })
                .map(|xy| (xy.x.round() as i32, xy.y.round() as i32))
                .unwrap_or((0, 0));
            let player_tile_center = scene
                .entities_with_component::<LocalTransform>()
                .into_iter()
                .find(|entity| scene.entity_name(*entity) == Some("player"))
                .and_then(|player| {
                    scene
                        .get_component::<LocalTransform>(player)
                        .map(|transform| transform.position)
                })
                .and_then(|player_pos| {
                    let tilemap_entity = scene
                        .entities_with_component::<TilemapComponent>()
                        .into_iter()
                        .next()?;
                    let tilemap = scene.get_component::<TilemapComponent>(tilemap_entity)?;
                    let tilemap_global = scene.get_component::<GlobalTransform>(tilemap_entity)?;
                    let world_to_object = tilemap_global.object_to_world.inverse();
                    let player_object_pos = world_to_object.transform_point3(player_pos);
                    let tile_x = (player_object_pos.x / tilemap.tileset.tile_size.x).round() as i32;
                    let tile_y = (player_object_pos.y / tilemap.tileset.tile_size.y).round() as i32;
                    if tile_x < 0 || tile_y < 0 {
                        return None;
                    }
                    let tile = glam::UVec2::new(tile_x as u32, tile_y as u32)
                        .min(tilemap.size().saturating_sub(glam::UVec2::ONE));
                    let center_object = tilemap.tile_to_object_position(tile);
                    let center_world = tilemap_global
                        .object_to_world
                        .transform_point3(center_object);
                    Some((center_world.x.round() as i32, center_world.y.round() as i32))
                })
                .unwrap_or((0, 0));
            let frame_ms = ctx
                .last_frame_timings
                .map(|t| t.total_frame_time.as_secs_f32() * 1000.0)
                .unwrap_or(0.0);
            let fps = if frame_ms > 0.0 {
                1000.0 / frame_ms
            } else {
                0.0
            };
            let str = format!(
                "{:>20} FPS | {:>20} ms | dt = {:?}\nPlayer: ({}, {}) | Tile center: ({}, {})\n\nRenderer stats: {:?}\n\nTimings: {:?}\n\nDebug layers: {:08b}",
                fps,
                frame_ms,
                dt,
                player_xy.0,
                player_xy.1,
                player_tile_center.0,
                player_tile_center.1,
                ctx.last_frame_stats,
                ctx.last_frame_timings,
                *ctx.shown_debug_layers & 0xFF,
            );
            if let Some(label) = self.debug_label_entity
                && let Some(text) = scene.get_component_mut::<TextComponent>(label)
            {
                text.set_text(str);
            }
        }
    }

    fn resolve_audio_handle(
        ctx: &mut UpdateCtx<'_>,
        path: Option<&str>,
    ) -> Option<AssetHandle<Audio>> {
        match path {
            Some(path) => Some(ctx.assets.get_handle::<Audio>(path).unwrap()),
            None => None,
        }
    }

    fn spawn_trigger(scene: &mut Scene, trigger: &TriggerObjectDef, index: usize) -> LevelTrigger {
        let trigger_entity = scene.spawn_spatial_entity(LocalTransform::from_xyz(
            trigger.position.0,
            trigger.position.1,
            0.0,
        ));
        let name = trigger
            .name
            .as_deref()
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("trigger_{index}"));
        scene.set_entity_name(trigger_entity, &name);
        let mut trigger_collider = TriggerCollider::new(CollisionShape::Rect {
            position: Vec2::ZERO,
            size: Vec2::new(trigger.size.0, trigger.size.1),
        });
        trigger_collider.layer = COL_LAYER_TRIGGERS;
        trigger_collider.collides_with = COL_LAYER_PLAYER | COL_LAYER_ENEMIES;
        scene.add_component(
            trigger_entity,
            ColliderComponent::new_single(Collider::Trigger(trigger_collider)),
        );

        LevelTrigger {
            entity: trigger_entity,
            action: trigger.action.clone(),
        }
    }

    fn interaction_button_label(
        control_type: ControlType,
        locales: &LocaleManager,
        input: &InputManager,
    ) -> String {
        locales.get_prompt_for_action(input, "overtake", control_type)
    }

    fn interaction_prompt_text(
        &self,
        scene: &Scene,
        action: InteractionAction,
        entity: Option<EntityHandle>,
        control_type: ControlType,
        locales: &LocaleManager,
        input: &InputManager,
    ) -> String {
        let button = Self::interaction_button_label(control_type, locales, input);
        let key = match action {
            InteractionAction::BeginOvertakeEnemy => "interaction.overtake",
            InteractionAction::BreakObstacle => "interaction.break",
            InteractionAction::VentAccess => {
                if self.player_in_vent {
                    "interaction.exit_vent"
                } else if let Some(vent_entity) = entity {
                    let vent_open = scene
                        .get_component::<VentEntranceComponent>(vent_entity)
                        .map(|component| component.is_open())
                        .unwrap_or(true);
                    if vent_open {
                        "interaction.enter_vent"
                    } else {
                        "interaction.open_vent"
                    }
                } else {
                    "interaction.enter_vent"
                }
            }
        };
        let main_prompt = locales.get_string(key);
        format!("{} {}", main_prompt, button)
    }

    fn update_vent_layer_visibility(&self, scene: &mut Scene) {
        if let Some(vent_tilemap) = self.vent_tilemap_entity {
            scene.set_visible(vent_tilemap, self.player_in_vent);
        }
    }

    fn toggle_vent_state(&mut self, scene: &mut Scene) {
        self.player_in_vent = !self.player_in_vent;
        self.update_vent_layer_visibility(scene);
        self.refresh_player_state(scene);
    }

    fn snap_player_to_entity(&self, scene: &mut Scene, target: EntityHandle) {
        let Some(player) = self.player_entity else {
            return;
        };
        let Some(target_position) = scene
            .get_component::<LocalTransform>(target)
            .map(|transform| transform.position.truncate())
        else {
            return;
        };
        if let Some(player_transform) = scene.get_component_mut::<LocalTransform>(player) {
            player_transform.position.x = target_position.x;
            player_transform.position.y = target_position.y;
        }
    }

    fn handle_vent_access(
        &mut self,
        ctx: &mut UpdateCtx,
        scene: &mut Scene,
        vent_entity: EntityHandle,
    ) {
        let Some(vent_component) = scene.get_component::<VentEntranceComponent>(vent_entity) else {
            return;
        };
        let vent_open = vent_component.is_open();
        if !vent_open && self.player_can_open_vents(scene) && !self.player_in_vent {
            let sfx = ctx
                .assets
                .get_handle::<Audio>("assets/audio/sfx/vent_open.ogg")
                .unwrap();
            MapSceneLogic::play_sfx(ctx, sfx);
            self.set_vent_open(scene, vent_entity, true);
            return;
        }
        if !vent_component.is_open() {
            return;
        }
        if self.player_can_open_vents(scene) {
            return;
        }
        self.toggle_vent_state(scene);
        self.snap_player_to_entity(scene, vent_entity);
    }

    fn set_vent_open(&self, scene: &mut Scene, vent_entity: EntityHandle, open: bool) {
        let mut vent_material = None;
        if let Some(vent_component) = scene.get_component_mut::<VentEntranceComponent>(vent_entity)
        {
            vent_component.set_open(open);
            vent_material = Some(vent_component.current_material());
        }
        if let Some(material) = vent_material {
            if let Some(sprite) = scene.get_component_mut::<SpriteComponent>(vent_entity) {
                sprite.material = material;
            }
        }
        if let Some(collider_component) = scene.get_component_mut::<ColliderComponent>(vent_entity)
        {
            for collider in collider_component.colliders_mut() {
                if let Collider::Static(static_collider) = collider {
                    static_collider.collides_with =
                        if open { 0 } else { GAME_COLLIDES_WITH_DEFAULT };
                }
            }
        }
    }

    fn player_can_open_vents(&self, scene: &Scene) -> bool {
        let Some(session) = self.overtake_session.as_ref() else {
            return false;
        };
        scene
            .get_component::<EnemyComponent>(session.enemy)
            .map(|enemy| enemy.can_open_vents())
            .unwrap_or(false)
    }

    fn current_player_lighting_layer(&self) -> u8 {
        if self.player_in_vent {
            LIGHTING_LAYER_VENTS
        } else {
            LIGHTING_LAYER_GROUND
        }
    }

    fn store_player_collision_mask(&mut self, scene: &Scene) {
        if let Some(player) = self.player_entity {
            if let Some(collider_component) = scene.get_component::<ColliderComponent>(player) {
                if let Some(mask) =
                    collider_component
                        .colliders()
                        .iter()
                        .find_map(|collider| match collider {
                            Collider::Dynamic(dynamic) => Some(dynamic.collides_with),
                            _ => None,
                        })
                {
                    self.player_collides_with_mask = mask;
                }
            }
        }
    }

    fn set_player_collision_mask(&self, scene: &mut Scene, mask: u32) {
        if let Some(player) = self.player_entity {
            if let Some(collider_component) = scene.get_component_mut::<ColliderComponent>(player) {
                for collider in collider_component.colliders_mut() {
                    if let Collider::Dynamic(dynamic) = collider {
                        dynamic.collides_with = mask;
                    }
                }
            }
        }
    }

    fn set_player_collision_layer(&self, scene: &mut Scene, layer: u32) {
        if let Some(player) = self.player_entity {
            if let Some(collider_component) = scene.get_component_mut::<ColliderComponent>(player) {
                for collider in collider_component.colliders_mut() {
                    if let Collider::Dynamic(dynamic) = collider {
                        dynamic.layer = layer;
                    }
                }
            }
        }
    }

    fn set_player_z(&self, scene: &mut Scene, z: f32) {
        if let Some(player) = self.player_entity {
            if let Some(transform) = scene.get_component_mut::<LocalTransform>(player) {
                transform.position.z = z;
            }
        }
    }

    fn apply_player_lighting_layer(&self, scene: &mut Scene, layer: u8) {
        if let Some(player_light) = self.player_light_entity {
            if let Some(light) = scene.get_component_mut::<LightSourceComponent>(player_light) {
                light.lighting_layer = layer;
            }
        }
        if let Some(spotlight) = self.player_spotlight_pivot_entity {
            if let Some(light) = scene.get_component_mut::<LightSourceComponent>(spotlight) {
                light.lighting_layer = layer;
            }
        }
        if let Some(player) = self.player_entity {
            if let Some(sprite) = scene.get_component_mut::<AnimatedSpriteComponent>(player) {
                sprite.set_lighting_layer(Some(layer));
            }
        }
    }

    fn refresh_player_state(&self, scene: &mut Scene) {
        let (layer, mask) = if self.player_in_vent {
            (COL_LAYER_VENTS, COL_LAYER_VENTS)
        } else {
            (COL_LAYER_PLAYER, self.player_collides_with_mask)
        };
        self.set_player_collision_layer(scene, layer);
        self.set_player_collision_mask(scene, mask);
        self.apply_player_lighting_layer(scene, self.current_player_lighting_layer());
        let z = if self.player_in_vent {
            Z_PLAYER_VENTS
        } else {
            Z_PLAYER_GROUND
        };
        self.set_player_z(scene, z);
        if let Some(player) = self.player_entity {
            if let Some(player_component) = scene.get_component_mut::<PlayerComponent>(player) {
                player_component.set_detectable(!self.player_in_vent);
            }
        }
    }

    fn kill_player(&mut self, ctx: &mut UpdateCtx<'_>, scene: &mut Scene, player: EntityHandle) {
        if let Some(pending) = self.pending_overtake.take()
            && let Some(enemy_component) = scene.get_component_mut::<EnemyComponent>(pending.enemy)
        {
            enemy_component.set_frozen(false);
        }
        self.pending_overtake = None;
        self.overtake_session = None;
        if let Some(spotlight_pivot) = self.player_spotlight_pivot_entity {
            scene.set_visible(spotlight_pivot, false);
        }
        if let Some(player_light) = self.player_light_entity {
            scene.set_visible(player_light, true);
        }
        if let Some(player_component) = scene.get_component_mut::<PlayerComponent>(player) {
            player_component.set_detectable(false);
            player_component.set_dead(true);
        }
        ctx.inputs
            .request_rumble(RumbleMotor::Both, Duration::from_secs_f32(3.0));
        for enemy in scene.entities_with_component::<EnemyComponent>() {
            let enemy_pos = scene
                .get_component::<LocalTransform>(enemy)
                .map(|transform| transform.position.truncate());
            if let Some(enemy_pos) = enemy_pos
                && let Some(enemy_component) = scene.get_component_mut::<EnemyComponent>(enemy)
            {
                enemy_component.stop_pursuit(enemy_pos);
            }
        }
        self.player_death_remaining_s = Some(PLAYER_DEATH_RESTART_DELAY_S);
        self.update_progress_bar(scene, 0.0);
    }

    fn player_should_die(&self, scene: &Scene, player: EntityHandle) -> bool {
        if self.player_in_vent {
            return false;
        }
        if scene
            .get_component::<PlayerComponent>(player)
            .is_some_and(PlayerComponent::is_invincible)
        {
            return false;
        }
        let Some(player_pos) = self.entity_interaction_center(scene, player) else {
            return false;
        };
        let max_dist_sq = PLAYER_KILL_CONTACT_PX * PLAYER_KILL_CONTACT_PX;
        scene
            .entities_with_component::<EnemyComponent>()
            .into_iter()
            .any(|enemy| {
                let Some(enemy_component) = scene.get_component::<EnemyComponent>(enemy) else {
                    return false;
                };
                if enemy_component.state() != EnemyState::Alerted {
                    return false;
                }
                let Some(enemy_pos) = self.entity_interaction_center(scene, enemy) else {
                    return false;
                };
                enemy_pos.distance_squared(player_pos) <= max_dist_sq
            })
    }

    fn entity_interaction_center(&self, scene: &Scene, entity: EntityHandle) -> Option<Vec2> {
        if let Some(global) = scene.get_component::<GlobalTransform>(entity)
            && let Some(collider_component) = scene.get_component::<ColliderComponent>(entity)
            && let Some(collider) = collider_component.colliders().first()
        {
            return Some(
                collider
                    .shape()
                    .transform(global.object_to_world)
                    .position(),
            );
        }
        if let Some(global) = scene.get_component::<GlobalTransform>(entity) {
            return Some(
                global
                    .object_to_world
                    .transform_point3(Vec3::ZERO)
                    .truncate(),
            );
        }
        scene
            .get_component::<LocalTransform>(entity)
            .map(|transform| transform.position.truncate())
    }

    fn update_progress_bar(&self, scene: &mut Scene, remaining_s: f32) {
        let is_overtaking = self.overtake_session.is_some();
        let Some(empty_entity) = self.overtake_bar_empty_entity else {
            return;
        };
        let Some(fill_entity) = self.overtake_bar_fill_entity else {
            return;
        };

        // Bar is only visible while overtaking an enemy.
        scene.set_visible(empty_entity, is_overtaking);
        if !is_overtaking {
            scene.set_visible(fill_entity, false);
            return;
        }

        let overtake_duration_s = self
            .overtake_session
            .as_ref()
            .map(|session| session.overtake_duration_s)
            .unwrap_or(1.0);
        let ratio_remaining = (remaining_s / overtake_duration_s).clamp(0.0, 1.0);

        // Show the "full" texture clipped from the right as overtake progresses (ratio decreases).
        if let Some(fill_sprite) = scene.get_component_mut::<SpriteComponent>(fill_entity) {
            fill_sprite.size = Vec2::new(OVERTAKE_BAR_W_PX * ratio_remaining, OVERTAKE_BAR_H_PX);
            fill_sprite.uv_min = Vec2::ZERO;
            fill_sprite.uv_max = Vec2::new(ratio_remaining, 1.0);
        }

        if let Some(fill_transform) = scene.get_component_mut::<LocalTransform>(fill_entity) {
            // Keep the left edge aligned with the empty bar's left edge.
            fill_transform.position.x =
                -(OVERTAKE_BAR_W_PX - (OVERTAKE_BAR_W_PX * ratio_remaining)) * 0.5;
        }

        scene.set_visible(fill_entity, ratio_remaining > 0.0);
    }

    fn restart_transition(&self, ctx: &mut UpdateCtx<'_>) -> Option<SceneTransition> {
        let level_data = match LevelData::from_id(&self.level_id) {
            Ok(level_data) => level_data,
            Err(err) => {
                error!(
                    level.id = self.level_id,
                    ?err,
                    "Failed to reload level data"
                );
                return None;
            }
        };
        let level = match level_data.load(ctx.assets) {
            Ok(level) => level,
            Err(err) => {
                error!(
                    level.id = self.level_id,
                    ?err,
                    "Failed to rebuild level for restart"
                );
                return None;
            }
        };
        Some(SceneTransition::Replace(Box::new(
            MapSceneLogic::new(level, self.level_id.clone(), self.game_context.clone())
                .with_activated_checkpoint(self.activated_checkpoint)
                .with_elapsed(if self.activated_checkpoint.is_some() {
                    self.elapsed
                } else {
                    Duration::ZERO
                }),
        )))
    }

    fn load_enemy_types(path: &str) -> eyre::Result<EnemyTypes> {
        let raw = fs::read_to_string(path)?;
        let map: EnemyTypes = serde_json::from_str(&raw)?;
        Ok(map)
    }

    fn load_animation_definition(path: &str) -> eyre::Result<AnimatedSpriteDefinition> {
        let raw = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    fn make_animated_sprite(
        assets: &AssetManager,
        material: Material,
        size: Vec2,
        definition: &AnimatedSpriteDefinition,
    ) -> AnimatedSpriteComponent {
        let texture = assets
            .resolve_handle::<Texture>(material.texture)
            .expect("animated sprite material texture must be loaded");
        AnimatedSpriteComponent::new(material, size, texture.size, definition.clone())
            .with_lighting_layer(LIGHTING_LAYER_GROUND)
    }

    fn sync_player_animation(
        scene: &mut Scene,
        player: EntityHandle,
        movement: Vec2,
        is_overtaking: bool,
        is_dead: bool,
        dt: Duration,
    ) {
        let Some(anim) = scene.get_component_mut::<AnimatedSpriteComponent>(player) else {
            return;
        };

        if is_dead {
            anim.set_state("dead");
            anim.set_moving(false);
        } else if is_overtaking {
            anim.set_state("overtake");
            anim.set_moving(false);
        } else {
            anim.set_state("default");
            anim.set_moving(movement != Vec2::ZERO);
            anim.set_facing_from_direction(movement);
        }

        anim.advance(dt);
    }

    fn spawn_player(
        ctx: &mut UpdateCtx<'_>,
        scene: &mut Scene,
        blorp_mat: Material,
        blorp_anim: &AnimatedSpriteDefinition,
        spawn_pos: Vec2,
    ) -> (EntityHandle, EntityHandle, EntityHandle) {
        let player_entity = scene.spawn_spatial_entity(LocalTransform::from_xyz(
            spawn_pos.x,
            spawn_pos.y,
            Z_PLAYER_GROUND,
        ));
        let player_sprite =
            Self::make_animated_sprite(ctx.assets, blorp_mat, Vec2::new(48.0, 16.0), blorp_anim);
        scene.add_component(player_entity, player_sprite);
        scene.set_entity_name(player_entity, "player");

        let mut player_collider = DynamicCollider::new(
            CollisionShape::Circle {
                position: Vec2::new(0.0, 0.0),
                radius: 5.0,
            },
            100.0,
        );
        player_collider.layer = COL_LAYER_PLAYER;
        player_collider.collides_with = GAME_DYNAMIC_COLLIDES_WITH_DEFAULT;
        scene.add_component(
            player_entity,
            ColliderComponent::new_single(Collider::Dynamic(player_collider)),
        );
        scene.add_component(player_entity, PlayerComponent::default());

        let player_light_entity = scene.spawn_spatial_entity(LocalTransform::from_xyz(
            spawn_pos.x,
            spawn_pos.y,
            Z_PLAYER_GROUND - 1.0,
        ));
        scene.add_component(
            player_light_entity,
            LightSourceComponent::new_pointlight(
                Vec2::new(0.0, 0.0),
                180.0,
                Color::from_rgba_u8(255, 255, 255, 64),
                0.5,
            )
            .with_lighting_layer(LIGHTING_LAYER_GROUND),
        );

        let spotlight_pivot = scene.spawn_spatial_entity(LocalTransform::from_xyz(0.0, 0.0, 0.0));
        scene.set_parent(spotlight_pivot, Some(player_entity));
        scene.add_component(
            spotlight_pivot,
            LightSourceComponent::new_spotlight(
                Vec2::new(-3.0, 0.0),
                180.0,
                Color::from_rgba_u8(0, 255, 0, 128),
                2.0,
                Vec2::NEG_X,
                std::f32::consts::PI / 3.0,
            )
            .with_lighting_layer(LIGHTING_LAYER_GROUND),
        );

        (player_entity, player_light_entity, spotlight_pivot)
    }

    fn set_checkpoint_entity_active(
        scene: &mut Scene,
        checkpoint_entity: EntityHandle,
        active: bool,
    ) {
        let mut material = None;
        if let Some(checkpoint) = scene.get_component_mut::<CheckpointComponent>(checkpoint_entity)
        {
            checkpoint.active = active;
            material = Some(if active {
                checkpoint.active_material
            } else {
                checkpoint.inactive_material
            });
        }
        if let Some(material) = material
            && let Some(sprite) = scene.get_component_mut::<SpriteComponent>(checkpoint_entity)
        {
            sprite.material = material;
        }
    }

    fn activate_checkpoint(&mut self, scene: &mut Scene, checkpoint_entity: EntityHandle) {
        if !scene.is_entity_alive(checkpoint_entity) {
            return;
        }
        let center = scene
            .get_component::<CheckpointComponent>(checkpoint_entity)
            .map(|checkpoint| checkpoint.center);
        for entity in self.checkpoints.iter().copied() {
            if !scene.is_entity_alive(entity) {
                continue;
            }
            Self::set_checkpoint_entity_active(scene, entity, entity == checkpoint_entity);
        }
        self.activated_checkpoint = center;
    }

    fn spawn_checkpoint(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        scene: &mut Scene,
        checkpoint: &CheckpointObjectDef,
        index: usize,
    ) {
        let checkpoint_tex = ctx
            .assets
            .get_handle::<Texture>("assets/textures/checkpoint.png")
            .unwrap();
        let checkpoint_tex_emis = ctx
            .assets
            .get_handle::<Texture>("assets/textures/checkpointemis.png")
            .unwrap();
        let checkpoint_active_tex = ctx
            .assets
            .get_handle::<Texture>("assets/textures/checkpoint_active.png")
            .unwrap();
        let checkpoint_active_tex_emis = ctx
            .assets
            .get_handle::<Texture>("assets/textures/checkpoint_activeemis.png")
            .unwrap();
        let inactive_material =
            Material::new_alpha(checkpoint_tex).with_emissive(checkpoint_tex_emis);
        let active_material =
            Material::new_alpha(checkpoint_active_tex).with_emissive(checkpoint_active_tex_emis);
        let center = Vec2::new(checkpoint.position.0, checkpoint.position.1);
        let starts_active = self
            .activated_checkpoint
            .is_some_and(|current| current.distance_squared(center) <= 0.001);
        let material = if starts_active {
            active_material
        } else {
            inactive_material
        };

        let checkpoint_entity = scene.spawn_sprite_entity(
            LocalTransform::from_xyz(
                checkpoint.position.0,
                checkpoint.position.1,
                Z_OBJECTS_ENEMIES,
            ),
            SpriteComponent::new(material, Vec2::splat(CHECKPOINT_SIZE_PX))
                .with_lighting_layer(LIGHTING_LAYER_GROUND),
        );
        scene.set_entity_name(checkpoint_entity, &format!("checkpoint_{index}"));
        scene.add_component(
            checkpoint_entity,
            CheckpointComponent {
                inactive_material,
                active_material,
                center,
                active: starts_active,
            },
        );

        let mut trigger = TriggerCollider::new(CollisionShape::Rect {
            position: Vec2::ZERO,
            size: Vec2::splat(CHECKPOINT_SIZE_PX),
        });
        trigger.layer = COL_LAYER_TRIGGERS;
        trigger.collides_with = COL_LAYER_PLAYER;
        scene.add_component(
            checkpoint_entity,
            ColliderComponent::new_single(Collider::Trigger(trigger)),
        );
        self.checkpoints.push(checkpoint_entity);
    }

    fn spawn_enemy(
        ctx: &mut UpdateCtx<'_>,
        scene: &mut Scene,
        enemy_types: &EnemyTypes,
        animation_defs: &HashMap<String, AnimatedSpriteDefinition>,
        enemy: &EnemyObjectDef,
        index: usize,
        prompt_text_style: TextStyle,
        prompt_text_layout: TextLayout,
    ) {
        let patrol = enemy
            .patrol
            .iter()
            .map(|step| match step {
                PatrolStepDef::GoTo { go_to } => PatrolStep::GoTo(Vec2::new(go_to.0, go_to.1)),
                PatrolStepDef::Wait { wait } => PatrolStep::Wait {
                    duration: Duration::from_secs_f32(wait.duration_s.max(0.0)),
                    angle: wait.angle,
                },
            })
            .collect::<Vec<_>>();
        let enemy_cfg = enemy_types
            .get(enemy.enemy_type.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "Enemy type '{}' is not defined in {}",
                    enemy.enemy_type, ENEMY_TYPES_PATH
                )
            });

        let enemy_entity = scene.spawn_spatial_entity(LocalTransform::from_xyz(
            enemy.position.0,
            enemy.position.1,
            Z_OBJECTS_ENEMIES,
        ));
        let alert_sound = Self::resolve_audio_handle(ctx, enemy_cfg.alert_sound.as_deref());
        let possess_sound = Self::resolve_audio_handle(ctx, enemy_cfg.possess_sound.as_deref());
        let leave_sound = Self::resolve_audio_handle(ctx, enemy_cfg.leave_sound.as_deref());
        let name = enemy
            .name
            .as_deref()
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("enemy_{index}"));
        scene.set_entity_name(enemy_entity, &name);
        let can_open_vents = enemy_cfg.tags.iter().any(|tag| tag == "can_open_vents");
        scene.add_component(
            enemy_entity,
            EnemyComponent::new(
                enemy_cfg,
                patrol,
                COL_LAYER_WALLS,
                can_open_vents,
                alert_sound,
                possess_sound,
                leave_sound,
            ),
        );
        let mut enemy_collider = DynamicCollider::new(
            CollisionShape::Circle {
                position: Vec2::ZERO,
                radius: 5.0,
            },
            10000.0,
        );
        enemy_collider.layer = COL_LAYER_ENEMIES;
        let mut enemy_collides_with =
            enemy_cfg.collides_with_mask() & GAME_DYNAMIC_COLLIDES_WITH_DEFAULT;
        enemy_collides_with |= COL_LAYER_TRIGGERS;
        enemy_collider.collides_with = enemy_collides_with;
        scene.add_component(
            enemy_entity,
            ColliderComponent::new_single(Collider::Dynamic(enemy_collider)),
        );
        let enemy_viewcone = scene.spawn_spatial_entity(LocalTransform::from_xyz(0.0, 0.0, 0.0));
        scene.set_entity_name(enemy_viewcone, ENEMY_VIEWCONE_NAME);
        scene.set_parent(enemy_viewcone, Some(enemy_entity));
        scene.add_component(
            enemy_viewcone,
            LightSourceComponent::new_spotlight(
                Vec2::ZERO,
                100.0,
                Color::RED,
                2.0,
                Vec2::X,
                std::f32::consts::PI / 2.7,
            ),
        );
        if let Some(animation_path) = enemy_cfg.animation.as_deref() {
            let animation = animation_defs
                .get(animation_path)
                .unwrap_or_else(|| panic!("Animation '{}' is not loaded", animation_path));
            let material = enemy_cfg.material.resolve_material(ctx.assets).unwrap();
            let sprite = Self::make_animated_sprite(
                ctx.assets,
                material,
                Vec2::new(enemy_cfg.sprite_size.0, enemy_cfg.sprite_size.1),
                animation,
            );
            scene.add_component(enemy_entity, sprite);
        } else {
            let material = enemy_cfg.material.resolve_material(ctx.assets).unwrap();
            scene.add_component(
                enemy_entity,
                SpriteComponent::new(
                    material,
                    Vec2::new(enemy_cfg.sprite_size.0, enemy_cfg.sprite_size.1),
                )
                .with_lighting_layer(LIGHTING_LAYER_GROUND)
                .with_uv(
                    Vec2::new(enemy_cfg.uv_min.0, enemy_cfg.uv_min.1),
                    Vec2::new(enemy_cfg.uv_max.0, enemy_cfg.uv_max.1),
                ),
            );
        }
        scene.add_component(
            enemy_entity,
            InteractableComponent::new(
                InteractionRequirements {
                    overtaking: OvertakingRequirement::Forbidden,
                    required_tags: Vec::new(),
                },
                InteractionAction::BeginOvertakeEnemy,
            ),
        );

        let overtake_prompt = scene.spawn_text_entity(
            LocalTransform::from_xyz(0.0, -20.0, 4.0),
            TextComponent::new("", prompt_text_style, prompt_text_layout),
        );
        scene.set_entity_name(overtake_prompt, INTERACT_PROMPT_NAME);
        scene.set_parent(overtake_prompt, Some(enemy_entity));
    }

    fn spawn_obstacle(
        ctx: &mut UpdateCtx<'_>,
        scene: &mut Scene,
        obstacle: &ObstacleObjectDef,
        index: usize,
        prompt_text_style: TextStyle,
        prompt_text_layout: TextLayout,
    ) {
        let material = obstacle.material.resolve_material(ctx.assets).unwrap();
        let obstacle_entity = scene.spawn_sprite_entity(
            LocalTransform::from_xyz(obstacle.position.0, obstacle.position.1, Z_OBJECTS_ENEMIES),
            SpriteComponent::new(material, Vec2::new(16.0, 16.0))
                .with_lighting_layer(LIGHTING_LAYER_GROUND),
        );
        let name = obstacle
            .name
            .as_deref()
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("obstacle_{index}"));
        scene.set_entity_name(obstacle_entity, &name);
        let mut collider = StaticCollider::new(CollisionShape::Rect {
            position: Vec2::ZERO,
            size: Vec2::new(16.0, 16.0),
        });
        collider.layer = COL_LAYER_WALLS;
        collider.collides_with = GAME_COLLIDES_WITH_DEFAULT;
        scene.add_component(
            obstacle_entity,
            ColliderComponent::new_single(Collider::Static(collider)),
        );
        scene.add_component(
            obstacle_entity,
            InteractableComponent::new(
                InteractionRequirements {
                    overtaking: OvertakingRequirement::Required,
                    required_tags: obstacle.required_tags.clone(),
                },
                InteractionAction::BreakObstacle,
            ),
        );

        let prompt = scene.spawn_text_entity(
            // Keep vent prompt above player while venting (vent entrance is Z_VENT_ENTRANCE).
            LocalTransform::from_xyz(0.0, -16.0, 20.0),
            TextComponent::new("", prompt_text_style, prompt_text_layout),
        );
        scene.set_entity_name(prompt, INTERACT_PROMPT_NAME);
        scene.set_parent(prompt, Some(obstacle_entity));
    }

    fn spawn_vent_entrance(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        scene: &mut Scene,
        vent: &VentEntranceObjectDef,
        index: usize,
        prompt_text_style: TextStyle,
        prompt_text_layout: TextLayout,
        start_open: bool,
    ) {
        let vent_closed_tex = ctx
            .assets
            .get_handle::<Texture>("assets/textures/vent_grate.png")
            .unwrap();
        let vent_open_tex = ctx
            .assets
            .get_handle::<Texture>("assets/textures/vent_grate_open.png")
            .unwrap();
        let vent_closed_mat = Material::new_alpha(vent_closed_tex);
        let vent_entity = scene.spawn_sprite_entity(
            LocalTransform::from_xyz(vent.position.0, vent.position.1, Z_VENT_ENTRANCE),
            SpriteComponent::new(vent_closed_mat, Vec2::new(16.0, 16.0))
                .with_lighting_layer(LIGHTING_LAYER_VENTS),
        );
        let name = vent
            .name
            .as_deref()
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("vent_entrance_{index}"));
        scene.set_entity_name(vent_entity, &name);

        let vent_open_mat = Material::new_alpha(vent_open_tex);
        scene.add_component(
            vent_entity,
            VentEntranceComponent::new(vent_open_mat, vent_closed_mat, start_open),
        );

        scene.add_component(
            vent_entity,
            InteractableComponent::new(
                InteractionRequirements {
                    overtaking: OvertakingRequirement::Any,
                    required_tags: Vec::new(),
                },
                InteractionAction::VentAccess,
            ),
        );
        self.set_vent_open(scene, vent_entity, start_open);

        let prompt = scene.spawn_text_entity(
            LocalTransform::from_xyz(0.0, -16.0, 20.0),
            TextComponent::new("", prompt_text_style, prompt_text_layout),
        );
        scene.set_entity_name(prompt, INTERACT_PROMPT_NAME);
        scene.set_parent(prompt, Some(vent_entity));
    }

    fn spawn_level_objects(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        scene: &mut Scene,
        blorp_mat: Material,
        blorp_anim: &AnimatedSpriteDefinition,
        enemy_types: &EnemyTypes,
        animation_defs: &HashMap<String, AnimatedSpriteDefinition>,
        objects: &[LevelObjectDef],
        prompt_text_style: TextStyle,
        prompt_text_layout: TextLayout,
    ) -> (EntityHandle, EntityHandle, EntityHandle, Vec<LevelTrigger>) {
        let player_spawn = objects.iter().find_map(|object| match object {
            LevelObjectDef::PlayerSpawn(player) => Some(player),
            _ => None,
        });
        let player_spawn = player_spawn.expect("level must define a player_spawn object");
        let spawn_pos = self
            .activated_checkpoint
            .unwrap_or_else(|| Vec2::new(player_spawn.position.0, player_spawn.position.1));
        let (player_entity, player_light_entity, player_spotlight_pivot_entity) =
            Self::spawn_player(ctx, scene, blorp_mat, blorp_anim, spawn_pos);

        let mut checkpoint_index = 0usize;
        let mut enemy_index = 0usize;
        let mut obstacle_index = 0usize;
        let mut vent_index = 0usize;
        let mut trigger_index = 0usize;
        let mut triggers = Vec::new();
        for object in objects {
            match object {
                LevelObjectDef::PlayerSpawn(_) => {}
                LevelObjectDef::Checkpoint(checkpoint) => {
                    self.spawn_checkpoint(ctx, scene, checkpoint, checkpoint_index);
                    checkpoint_index += 1;
                }
                LevelObjectDef::Enemy(enemy) => {
                    Self::spawn_enemy(
                        ctx,
                        scene,
                        enemy_types,
                        animation_defs,
                        enemy,
                        enemy_index,
                        prompt_text_style,
                        prompt_text_layout,
                    );
                    enemy_index += 1;
                }
                LevelObjectDef::Obstacle(obstacle) => {
                    Self::spawn_obstacle(
                        ctx,
                        scene,
                        obstacle,
                        obstacle_index,
                        prompt_text_style,
                        prompt_text_layout,
                    );
                    obstacle_index += 1;
                }
                LevelObjectDef::VentEntrance(vent) => {
                    self.spawn_vent_entrance(
                        ctx,
                        scene,
                        vent,
                        vent_index,
                        prompt_text_style,
                        prompt_text_layout,
                        vent.open,
                    );
                    vent_index += 1;
                }
                LevelObjectDef::Trigger(trigger) => {
                    triggers.push(Self::spawn_trigger(scene, trigger, trigger_index));
                    trigger_index += 1;
                }
            }
        }

        (
            player_entity,
            player_light_entity,
            player_spotlight_pivot_entity,
            triggers,
        )
    }

    fn current_overtaken_tags(&self, scene: &Scene) -> Vec<String> {
        let Some(session) = self.overtake_session.as_ref() else {
            return Vec::new();
        };
        scene
            .get_component::<EnemyComponent>(session.enemy)
            .map(|enemy| enemy.tags().to_vec())
            .unwrap_or_default()
    }

    fn find_interaction_candidate(
        &self,
        scene: &Scene,
        player: EntityHandle,
        is_overtaking: bool,
        active_tags: &[String],
    ) -> Option<EntityHandle> {
        let player_pos = self.entity_interaction_center(scene, player)?;

        scene
            .entities_with_component::<InteractableComponent>()
            .into_iter()
            .filter_map(|entity| {
                let interactable = scene.get_component::<InteractableComponent>(entity)?;
                if !interactable.requirements_met(is_overtaking, active_tags) {
                    return None;
                }
                let action = interactable.action();
                if self.player_in_vent && action != InteractionAction::VentAccess {
                    return None;
                }
                match action {
                    InteractionAction::BeginOvertakeEnemy => {
                        let enemy_component = scene.get_component::<EnemyComponent>(entity)?;
                        if !enemy_component.can_be_overtaken() {
                            return None;
                        }
                        let enemy_pos = self.entity_interaction_center(scene, entity)?;
                        let distance = player_pos.distance(enemy_pos);
                        (distance <= OVERTAKE_RANGE_PX).then_some((entity, distance))
                    }
                    InteractionAction::BreakObstacle => {
                        let obstacle_pos = self.entity_interaction_center(scene, entity)?;
                        let distance = player_pos.distance(obstacle_pos);
                        (distance <= OBSTACLE_INTERACT_RANGE_PX).then_some((entity, distance))
                    }
                    InteractionAction::VentAccess => {
                        let entrance_pos = self.entity_interaction_center(scene, entity)?;
                        let distance = player_pos.distance(entrance_pos);
                        let vent_open = scene
                            .get_component::<VentEntranceComponent>(entity)
                            .map(|component| component.is_open())
                            .unwrap_or(false);
                        if vent_open && is_overtaking {
                            return None;
                        }
                        let can_open_closed =
                            self.player_can_open_vents(scene) && !self.player_in_vent;
                        (vent_open && distance <= OBSTACLE_INTERACT_RANGE_PX)
                            .then_some((entity, distance))
                            .or_else(|| {
                                (can_open_closed && distance <= OBSTACLE_INTERACT_RANGE_PX)
                                    .then_some((entity, distance))
                            })
                    }
                }
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(entity, _)| entity)
    }

    fn update_interact_prompts(
        &self,
        scene: &mut Scene,
        candidate: Option<EntityHandle>,
        control_type: ControlType,
        input: &InputManager,
    ) {
        for entity in scene.entities_with_component::<InteractableComponent>() {
            let Some(prompt_entity) = scene.find_named_child(entity, INTERACT_PROMPT_NAME) else {
                continue;
            };
            let action = scene
                .get_component::<InteractableComponent>(entity)
                .map(InteractableComponent::action);
            let desired_text = if Some(entity) == candidate {
                match action {
                    Some(action) => {
                        let ctx = self.game_context.borrow();
                        self.interaction_prompt_text(
                            scene,
                            action,
                            Some(entity),
                            control_type,
                            &ctx.locales,
                            input,
                        )
                    }
                    None => String::new(),
                }
            } else {
                String::new()
            };
            if let Some(prompt_text) = scene.get_component_mut::<TextComponent>(prompt_entity) {
                if prompt_text.text() != desired_text {
                    prompt_text.set_text(&desired_text);
                }
            }
        }
    }

    fn break_obstacle(&mut self, scene: &mut Scene, obstacle: EntityHandle) {
        if let Some(prompt) = scene.find_named_child(obstacle, INTERACT_PROMPT_NAME) {
            scene.despawn_entity(prompt);
        }
        scene.despawn_entity(obstacle);
    }

    fn begin_overtake(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        scene: &mut Scene,
        player: EntityHandle,
        enemy: EntityHandle,
    ) {
        let (possess_sound, leave_sound) = scene
            .get_component::<EnemyComponent>(enemy)
            .map(|enemy_component| {
                (
                    enemy_component.possess_sound(),
                    enemy_component.leave_sound(),
                )
            })
            .unwrap_or((None, None));
        scene.set_visible(player, true);
        let player_collider = scene.remove_component::<ColliderComponent>(player);
        if let Some(player_component) = scene.get_component_mut::<PlayerComponent>(player) {
            player_component.set_detectable(false);
        }
        let player_pos = scene
            .get_component::<LocalTransform>(player)
            .map(|transform| transform.position.truncate());
        let enemy_pos = scene
            .get_component::<LocalTransform>(enemy)
            .map(|transform| transform.position.truncate());
        if let Some(anim) = scene.get_component_mut::<AnimatedSpriteComponent>(player) {
            anim.set_state("overtake");
            anim.set_moving(false);
            if let (Some(player_pos), Some(enemy_pos)) = (player_pos, enemy_pos) {
                anim.set_facing_from_direction(enemy_pos - player_pos);
            }
        }
        ctx.inputs
            .request_rumble(RumbleMotor::Strong, Duration::from_secs(1));
        if let Some(enemy_component) = scene.get_component_mut::<EnemyComponent>(enemy) {
            enemy_component.set_frozen(true);
        }

        self.pending_overtake = Some(PendingOvertake {
            enemy,
            player_collider,
            possess_sound,
            leave_sound,
        });
        self.update_progress_bar(scene, 0.0);
    }

    fn begin_overtake_session(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        scene: &mut Scene,
        player: EntityHandle,
    ) {
        let Some(pending) = self.pending_overtake.take() else {
            return;
        };

        let enemy_pos = scene
            .get_component::<LocalTransform>(pending.enemy)
            .map(|transform| transform.position.truncate());

        let Some(enemy_component) = scene.get_component::<EnemyComponent>(pending.enemy) else {
            if let Some(player_component) = scene.get_component_mut::<PlayerComponent>(player) {
                player_component.set_detectable(true);
            }
            if let Some(player_collider) = pending.player_collider {
                scene.add_component(player, player_collider);
            }
            if let Some(leave_sound) = pending.leave_sound {
                Self::play_sfx(ctx, leave_sound);
            }
            return;
        };

        let overtake_duration_s = enemy_component.overtake_duration_s();
        let overtaken_speed = enemy_component.overtaken_speed();
        if let Some(enemy_pos) = enemy_pos
            && let Some(player_transform) = scene.get_component_mut::<LocalTransform>(player)
        {
            player_transform.position.x = enemy_pos.x;
            player_transform.position.y = enemy_pos.y;
        }

        scene.set_visible(player, false);
        if let Some(enemy_component) = scene.get_component_mut::<EnemyComponent>(pending.enemy) {
            enemy_component.set_frozen(false);
            enemy_component.begin_overtaken();
        }
        self.overtake_session = Some(OvertakeSession {
            enemy: pending.enemy,
            remaining_s: overtake_duration_s,
            overtake_duration_s,
            overtaken_speed,
            player_collider: pending.player_collider,
            leave_sound: pending.leave_sound,
        });
        if let Some(possess_sound) = pending.possess_sound {
            Self::play_sfx(ctx, possess_sound);
        }
        self.update_progress_bar(scene, overtake_duration_s);
    }

    fn end_overtake(&mut self, ctx: &mut UpdateCtx<'_>, scene: &mut Scene, player: EntityHandle) {
        let Some(session) = self.overtake_session.take() else {
            return;
        };

        let mut player_spawn_pos = scene
            .get_component::<LocalTransform>(player)
            .map(|transform| transform.position.truncate())
            .unwrap_or(Vec2::ZERO);

        if scene.is_entity_alive(session.enemy) {
            if let Some(enemy_pos) = scene
                .get_component::<LocalTransform>(session.enemy)
                .map(|transform| transform.position.truncate())
            {
                player_spawn_pos = enemy_pos;
            }

            let enemy_collider = scene.remove_component::<ColliderComponent>(session.enemy);
            if let Some(enemy_component) = scene.get_component_mut::<EnemyComponent>(session.enemy)
            {
                enemy_component.enter_stunned(enemy_collider);
            }
        }

        if let Some(player_transform) = scene.get_component_mut::<LocalTransform>(player) {
            player_transform.position.x = player_spawn_pos.x;
            player_transform.position.y = player_spawn_pos.y;
        }
        scene.set_visible(player, true);
        if let Some(player_collider) = session.player_collider {
            scene.add_component(player, player_collider);
        }
        if let Some(player_component) = scene.get_component_mut::<PlayerComponent>(player) {
            player_component.set_detectable(true);
        }
        if let Some(leave_sound) = session.leave_sound {
            Self::play_sfx(ctx, leave_sound);
        }

        self.update_progress_bar(scene, 0.0);
    }
}

fn merge_static_tile_colliders(collision_layers: &Array2D<u32>, tile_size: Vec2) -> Vec<Collider> {
    let mut visited = Array2D::new(collision_layers.width(), collision_layers.height(), false);
    let mut colliders = Vec::new();

    for y in 0..collision_layers.height() {
        for x in 0..collision_layers.width() {
            let layer = *collision_layers.get(x, y);
            if layer == 0 || *visited.get(x, y) {
                continue;
            }

            let mut width = 1;
            while x + width < collision_layers.width()
                && *collision_layers.get(x + width, y) == layer
                && !*visited.get(x + width, y)
            {
                width += 1;
            }

            let mut height = 1;
            'grow_height: while y + height < collision_layers.height() {
                for dx in 0..width {
                    if *collision_layers.get(x + dx, y + height) != layer
                        || *visited.get(x + dx, y + height)
                    {
                        break 'grow_height;
                    }
                }
                height += 1;
            }

            for dy in 0..height {
                for dx in 0..width {
                    visited.set(x + dx, y + dy, true);
                }
            }

            let top_left = TilemapComponent::tile_to_object_position_with_size(
                UVec2::new(x as u32, y as u32),
                tile_size,
            )
            .truncate();
            let bottom_right = TilemapComponent::tile_to_object_position_with_size(
                UVec2::new((x + width - 1) as u32, (y + height - 1) as u32),
                tile_size,
            )
            .truncate();
            let position = top_left.midpoint(bottom_right);
            let size = Vec2::new(width as f32 * tile_size.x, height as f32 * tile_size.y);

            let mut collider = StaticCollider::new(CollisionShape::Rect { position, size });
            collider.layer = layer;
            colliders.push(Collider::Static(collider));
        }
    }

    colliders
}

impl GameSceneLogic for MapSceneLogic {
    fn on_enter(&mut self, scene: &mut Scene, ctx: &mut UpdateCtx<'_>) {
        self.overtake_session = None;
        self.player_death_remaining_s = None;
        self.checkpoints.clear();

        scene.set_lighting_layer_count(2);

        Self::play_music(ctx, "assets/audio/music/stealth.ogg");

        let blorp_tex = ctx
            .assets
            .get_handle::<Texture>("assets/textures/glorp.png")
            .unwrap();
        let blorp_mat = Material::new_opaque(blorp_tex);

        let ui_font = ctx.assets.get_handle::<Font>("Tiny5").unwrap();

        let sky_texture = ctx
            .assets
            .get_handle::<Texture>("assets/textures/background.png")
            .unwrap();
        let sky_material = Material::new_alpha(sky_texture);
        let mut transform_sky = LocalTransform::from_xyz(0.0, 0.0, Z_SKY);
        transform_sky.screen_space = true;
        let mut sprite_sky = SpriteComponent::new(
            sky_material,
            Vec2::new(
                BASE_VIRTUAL_WIDTH as f32 * 2.0,
                BASE_VIRTUAL_HEIGHT as f32 * 2.0,
            ),
        );
        sprite_sky.uv_min = Vec2::new(0.0, 0.0);
        sprite_sky.uv_max = Vec2::new(1.0, 1.0);
        self.sky_entity = Some(scene.spawn_sprite_entity(transform_sky, sprite_sky));

        let level = self
            .level
            .take()
            .expect("map scene entered without a loaded level");
        let Level {
            tileset,
            map,
            objects,
            vent_layer,
            shadow_collision_mask,
        } = level;
        let tileset_width = tileset.tile_count.x;
        let tile_size = tileset.tile_size;
        let collision_map = tileset.collisions.as_ref().unwrap();
        let mut collision_layers = Array2D::new(map.width(), map.height(), 0u32);
        let tilemap_map = Array2D::new_init(map.width(), map.height(), |x, y| {
            let index = map.get(x, y);
            if *index < 0 {
                None
            } else {
                let index = *index as u32;
                let (tx, ty) = (index % tileset_width, index / tileset_width);
                let collision_layer = *collision_map.get(tx as usize, ty as usize);
                if collision_layer != 0 {
                    collision_layers.set(x, y, collision_layer);
                }
                Some(UVec2::new(tx, ty))
            }
        });
        let mut colliders = merge_static_tile_colliders(&collision_layers, tile_size);
        for collider in colliders.iter_mut() {
            if let Collider::Static(static_collider) = collider {
                static_collider.collides_with = GAME_COLLIDES_WITH_DEFAULT;
            }
        }
        let mut tilemap = TilemapComponent::new(tileset, tilemap_map);
        tilemap.set_lighting_layer(Some(LIGHTING_LAYER_GROUND));
        tilemap.set_shadow_collision_mask(shadow_collision_mask);
        let tilemap_ent = scene.spawn_spatial_entity(LocalTransform::from_xyz(0.0, 0.0, Z_TILEMAP));
        let tilemap_transform = *scene.get_component::<GlobalTransform>(tilemap_ent).unwrap();
        let navigation_ground = NavigationComponent::from_tilemap_with_block_mask(
            &tilemap_transform,
            &tilemap,
            u32::MAX,
        );
        let navigation_flying = NavigationComponent::from_tilemap_with_block_mask(
            &tilemap_transform,
            &tilemap,
            !COL_LAYER_FLYING_IGNORED,
        );
        scene.add_component(tilemap_ent, tilemap);
        scene.add_component(tilemap_ent, ColliderComponent::new_multiple(colliders));
        let nav_ground_ent = scene.spawn_spatial_entity(LocalTransform::from_xyz(0.0, 0.0, 0.0));
        scene.set_entity_name(nav_ground_ent, NAVIGATION_GROUND_NAME);
        scene.add_component(nav_ground_ent, navigation_ground);
        let nav_flying_ent = scene.spawn_spatial_entity(LocalTransform::from_xyz(0.0, 0.0, 0.0));
        scene.set_entity_name(nav_flying_ent, NAVIGATION_FLYING_NAME);
        scene.add_component(nav_flying_ent, navigation_flying);

        let vent_tilemap_entity = if let Some(vent_layer) = vent_layer {
            let VentLayer {
                tileset: vent_tileset,
                map: vent_map,
            } = vent_layer;
            let vent_tilemap_map = Array2D::new_init(map.width(), map.height(), |x, y| {
                let tile_index = *vent_map.get(x, y);
                if tile_index < 0 {
                    None
                } else {
                    let tile_index = tile_index as u32;
                    let tx = tile_index % vent_tileset.tile_count.x;
                    let ty = tile_index / vent_tileset.tile_count.x;
                    Some(UVec2::new(tx, ty))
                }
            });
            let vent_collision_map = vent_tileset
                .collisions
                .as_ref()
                .expect("vent tileset missing collisions");
            let mut vent_collision_layers = Array2D::new(map.width(), map.height(), 0u32);
            let vent_tile_count = vent_tileset.tile_count;
            let vent_tile_size = vent_tileset.tile_size;
            for y in 0..map.height() {
                for x in 0..map.width() {
                    let tile_index = *vent_map.get(x, y);
                    if tile_index < 0 {
                        continue;
                    }
                    let tile_index = tile_index as u32;
                    let tx = (tile_index % vent_tile_count.x) as usize;
                    let ty = (tile_index / vent_tile_count.x) as usize;
                    let layer = *vent_collision_map.get(tx, ty);
                    if layer != 0 {
                        vent_collision_layers.set(x, y, layer);
                    }
                }
            }
            let mut vent_colliders =
                merge_static_tile_colliders(&vent_collision_layers, vent_tile_size);
            for collider in vent_colliders.iter_mut() {
                if let Collider::Static(static_collider) = collider {
                    static_collider.collides_with = COL_LAYER_VENTS;
                }
            }
            let mut vent_tilemap = TilemapComponent::new(vent_tileset, vent_tilemap_map);
            vent_tilemap.set_lighting_layer(Some(LIGHTING_LAYER_VENTS));
            let vent_entity =
                scene.spawn_spatial_entity(LocalTransform::from_xyz(0.0, 0.0, Z_VENTS_TILEMAP));
            scene.add_component(vent_entity, vent_tilemap);
            if !vent_colliders.is_empty() {
                scene.add_component(vent_entity, ColliderComponent::new_multiple(vent_colliders));
            }
            scene.set_visible(vent_entity, false);
            Some(vent_entity)
        } else {
            None
        };
        self.vent_tilemap_entity = vent_tilemap_entity;
        self.player_in_vent = false;
        self.update_vent_layer_visibility(scene);

        let enemy_types = Self::load_enemy_types(ENEMY_TYPES_PATH)
            .expect("Failed to load enemy type definitions");
        let blorp_anim = Self::load_animation_definition("assets/textures/glorp.anim.json")
            .expect("Failed to load player animation definition");
        let mut animation_defs = HashMap::new();
        for animation_path in enemy_types
            .values()
            .filter_map(|enemy| enemy.animation.as_deref())
        {
            animation_defs
                .entry(animation_path.to_string())
                .or_insert_with(|| {
                    Self::load_animation_definition(animation_path).unwrap_or_else(|err| {
                        panic!(
                            "Failed to load animation definition '{}': {}",
                            animation_path, err
                        )
                    })
                });
        }
        let (player_entity, player_light_entity, player_spotlight_pivot_entity, triggers) = self
            .spawn_level_objects(
                ctx,
                scene,
                blorp_mat,
                &blorp_anim,
                &enemy_types,
                &animation_defs,
                &objects,
                TextStyle {
                    font: ui_font,
                    line_height: None,
                    scale: 1.0,
                    color: [255, 255, 255, 255],
                },
                TextLayout {
                    max_width: Some(160.0),
                    max_height: None,
                    anchor: TextAnchor::HorizonCenter,
                },
            );
        self.player_entity = Some(player_entity);
        self.player_light_entity = Some(player_light_entity);
        self.player_spotlight_pivot_entity = Some(player_spotlight_pivot_entity);
        self.triggers = triggers;
        self.store_player_collision_mask(scene);
        self.refresh_player_state(scene);

        let bar_empty_tex = ctx
            .assets
            .get_handle::<Texture>(OVERTAKE_BAR_TEXTURE_EMPTY)
            .unwrap();
        let bar_full_tex = ctx
            .assets
            .get_handle::<Texture>(OVERTAKE_BAR_TEXTURE_FULL)
            .unwrap();
        let bar_empty_mat = Material::new_alpha(bar_empty_tex);
        let bar_full_mat = Material::new_alpha(bar_full_tex);

        // Position: top edge of the bar roughly 8 px under the top of the virtual resolution.
        let overtake_bar_y = -(BASE_VIRTUAL_HEIGHT as f32) * 0.5 + 8.0 + (OVERTAKE_BAR_H_PX * 0.5);

        let bar_empty = scene.spawn_sprite_entity(
            LocalTransform::from_xyz_screenspace(0.0, overtake_bar_y, Z_UI),
            SpriteComponent::new(
                bar_empty_mat,
                Vec2::new(OVERTAKE_BAR_W_PX, OVERTAKE_BAR_H_PX),
            ),
        );
        let bar_fill = scene.spawn_sprite_entity(
            LocalTransform::from_xyz_screenspace(0.0, overtake_bar_y, Z_UI + 1.0),
            SpriteComponent::new(
                bar_full_mat,
                Vec2::new(OVERTAKE_BAR_W_PX, OVERTAKE_BAR_H_PX),
            ),
        );
        self.overtake_bar_empty_entity = Some(bar_empty);
        self.overtake_bar_fill_entity = Some(bar_fill);
        self.update_progress_bar(scene, 0.0);
        self.timer_entity = Some(scene.spawn_text_entity(
            LocalTransform::from_xyz_screenspace(
                -(BASE_VIRTUAL_WIDTH as f32) * 0.5,
                -(BASE_VIRTUAL_HEIGHT as f32) * 0.5,
                Z_UI,
            ),
            TextComponent::new(
                "0:00.00",
                TextStyle {
                    font: ui_font,
                    line_height: None,
                    scale: 1.0,
                    color: [255, 255, 255, 255],
                },
                TextLayout {
                    max_width: None,
                    max_height: None,
                    anchor: TextAnchor::TopLeft,
                },
            ),
        ));

        if let Some(player_pos) = scene
            .get_component::<LocalTransform>(player_entity)
            .map(|transform| transform.position.truncate())
        {
            scene.camera_mut().center = player_pos;
        }
    }

    fn update(
        &mut self,
        scene: &mut Scene,
        dt: Duration,
        ctx: &mut UpdateCtx<'_>,
    ) -> SceneTransition {
        self.update_debug(scene, dt, ctx);
        if ctx.inputs.is_action_just_pressed("menu_pause") {
            if ctx.inputs.is_key_pressed(KeyboardKey::LeftControl) {
                // Ignore this pause trigger.
            } else {
                self.update_interact_prompts(
                    scene,
                    None,
                    ctx.inputs.last_control_type(),
                    &ctx.inputs,
                );
                return SceneTransition::Push(Box::new(PauseSceneLogic::new(
                    self.game_context.clone(),
                    false,
                )));
            }
        }
        if ctx.inputs.is_action_just_pressed("reset")
            && let Some(transition) = self.restart_transition(ctx)
        {
            return transition;
        }

        let Some(player) = self.player_entity else {
            return SceneTransition::None;
        };
        let dt_s = dt.as_secs_f32();

        if let Some(remaining_s) = self.player_death_remaining_s.as_mut() {
            *remaining_s = (*remaining_s - dt_s).max(0.0);
            Self::sync_player_animation(scene, player, Vec2::ZERO, false, true, dt);
            if *remaining_s <= f32::EPSILON {
                if let Some(transition) = self.restart_transition(ctx) {
                    return transition;
                }
            }
            return SceneTransition::None;
        }

        self.elapsed = self.elapsed.saturating_add(dt);

        if let Some(timer_entity) = self.timer_entity
            && let Some(text_comp) = scene.get_component_mut::<TextComponent>(timer_entity)
        {
            text_comp.set_text(if !self.show_text_debug_overlay {
                format_timer(self.elapsed)
            } else {
                "".to_string()
            });
        }

        if self.pending_overtake.is_some() {
            Self::sync_player_animation(scene, player, Vec2::ZERO, true, false, dt);
            let pending_finished = scene
                .get_component::<AnimatedSpriteComponent>(player)
                .is_some_and(AnimatedSpriteComponent::is_finished);
            if pending_finished {
                self.begin_overtake_session(ctx, scene, player);
            }
        } else {
            let movement = ctx.inputs.get_2d_axes_value("move_x", "move_y");

            if let Some(touched_checkpoint) = self.checkpoints.iter().copied().find(|checkpoint| {
                scene.is_entity_alive(*checkpoint) && scene.entities_collide(player, *checkpoint)
            }) {
                let already_active = scene
                    .get_component::<CheckpointComponent>(touched_checkpoint)
                    .map(|checkpoint| checkpoint.active)
                    .unwrap_or(false);
                if !already_active {
                    let sfx = ctx
                        .assets
                        .get_handle::<Audio>("assets/audio/sfx/checkpoint.ogg")
                        .unwrap();
                    MapSceneLogic::play_sfx(ctx, sfx);
                    self.activate_checkpoint(scene, touched_checkpoint);
                }
            }
            if self.overtake_session.is_none() && self.player_should_die(scene, player) {
                self.kill_player(ctx, scene, player);
                return SceneTransition::None;
            }

            let trigger_source = self
                .overtake_session
                .as_ref()
                .map(|session| session.enemy)
                .unwrap_or(player);
            for trigger in &self.triggers {
                if scene.is_entity_alive(trigger_source)
                    && scene.is_entity_alive(trigger.entity)
                    && scene.entities_collide(trigger_source, trigger.entity)
                {
                    if let Some(transition) = self.transition_for_trigger(trigger, ctx) {
                        return transition;
                    }
                }
            }

            let is_overtaking = self.overtake_session.is_some();
            let active_tags = self.current_overtaken_tags(scene);
            let candidate =
                self.find_interaction_candidate(scene, player, is_overtaking, &active_tags);
            self.update_interact_prompts(
                scene,
                candidate,
                ctx.inputs.last_control_type(),
                &ctx.inputs,
            );
            if let Some(candidate) = candidate
                && scene
                    .get_component::<InteractableComponent>(candidate)
                    .is_some_and(|c| c.action() == InteractionAction::BeginOvertakeEnemy)
            {
                // Sustained "you can overtake" hint.
                ctx.inputs
                    .request_rumble(RumbleMotor::Weak, Duration::from_secs_f32(0.1));
            }

            if ctx.inputs.is_action_just_pressed("overtake")
                && let Some(interactable) = candidate
                && let Some(action) = scene
                    .get_component::<InteractableComponent>(interactable)
                    .map(InteractableComponent::action)
            {
                match action {
                    InteractionAction::BeginOvertakeEnemy => {
                        self.begin_overtake(ctx, scene, player, interactable);
                    }
                    InteractionAction::BreakObstacle => {
                        ctx.inputs
                            .request_rumble(RumbleMotor::Both, Duration::from_secs_f32(0.5));
                        let sfx = ctx
                            .assets
                            .get_handle::<Audio>("assets/audio/sfx/crate_break.ogg")
                            .unwrap();
                        MapSceneLogic::play_sfx(ctx, sfx);
                        self.break_obstacle(scene, interactable);
                    }
                    InteractionAction::VentAccess => {
                        self.handle_vent_access(ctx, scene, interactable);
                    }
                }
            }

            if self.overtake_session.is_some() {
                let cancel_pressed = ctx.inputs.is_action_just_pressed("cancel");
                let mut should_end = false;
                let mut progress_remaining_s = None;

                {
                    let session = self.overtake_session.as_mut().expect("session exists");
                    if !scene.is_entity_alive(session.enemy) {
                        should_end = true;
                    } else {
                        let speed = session.overtaken_speed * dt_s;
                        if movement != Vec2::ZERO {
                            if let Some(enemy_transform) =
                                scene.get_component_mut::<LocalTransform>(session.enemy)
                            {
                                enemy_transform.position += movement.extend(0.0) * speed;
                            }
                            if let Some(enemy_component) =
                                scene.get_component_mut::<EnemyComponent>(session.enemy)
                            {
                                enemy_component.set_overtaken_facing(movement);
                            }
                        }

                        if let Some(enemy_pos) = scene
                            .get_component::<LocalTransform>(session.enemy)
                            .map(|transform| transform.position.truncate())
                            && let Some(player_transform) =
                                scene.get_component_mut::<LocalTransform>(player)
                        {
                            player_transform.position.x = enemy_pos.x;
                            player_transform.position.y = enemy_pos.y;
                        }

                        let rem_at_start = session.remaining_s;
                        session.remaining_s = (session.remaining_s - dt_s).max(0.0);
                        let rem_at_end = session.remaining_s;
                        progress_remaining_s = Some(session.remaining_s);

                        if 3.0 <= rem_at_start && 3.0 >= rem_at_end {
                            let warning = ctx
                                .assets
                                .get_handle::<Audio>("assets/audio/sfx/warning.ogg")
                                .unwrap();
                            MapSceneLogic::play_sfx(ctx, warning);
                        }

                        const VIBRATE_AT: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 3.0];

                        if VIBRATE_AT
                            .iter()
                            .any(|it| *it <= rem_at_start && *it >= rem_at_end)
                        {
                            ctx.inputs
                                .request_rumble(RumbleMotor::Weak, Duration::from_millis(100));
                        }

                        if session.remaining_s <= f32::EPSILON || cancel_pressed {
                            should_end = true;
                            ctx.inputs
                                .request_rumble(RumbleMotor::Strong, Duration::from_millis(500));
                        }
                    }
                }

                if let Some(remaining_s) = progress_remaining_s {
                    self.update_progress_bar(scene, remaining_s);
                }

                if should_end {
                    self.end_overtake(ctx, scene, player);
                }
            } else {
                if movement != Vec2::ZERO {
                    let speed = 130.0 * dt_s;
                    if let Some(player_trans) = scene.get_component_mut::<LocalTransform>(player) {
                        player_trans.position += movement.extend(0.0) * speed;
                    }
                    if let Some(spotlight_pivot) = self.player_spotlight_pivot_entity
                        && let Some(pivot_transform) =
                            scene.get_component_mut::<LocalTransform>(spotlight_pivot)
                    {
                        let target_dir = movement.normalize();
                        let rotate_speed_rad = 30.0;
                        let max_step = rotate_speed_rad * dt_s;
                        let current_angle = pivot_transform.rotation;
                        let target_angle = target_dir.y.atan2(target_dir.x) + std::f32::consts::PI;
                        let delta = (target_angle - current_angle + std::f32::consts::PI)
                            .rem_euclid(std::f32::consts::TAU)
                            - std::f32::consts::PI;
                        let step = delta.clamp(-max_step, max_step);
                        pivot_transform.rotation = current_angle + step;
                    }
                }
                self.update_progress_bar(scene, 0.0);
            }

            let is_overtaking = self.overtake_session.is_some();
            let is_dead = scene
                .get_component::<PlayerComponent>(player)
                .is_some_and(PlayerComponent::is_dead);
            Self::sync_player_animation(scene, player, movement, is_overtaking, is_dead, dt);
        }

        if let Some(player_pos) = scene
            .get_component::<LocalTransform>(player)
            .map(|transform| transform.position.truncate())
        {
            if let Some(player_light) = self.player_light_entity
                && let Some(light_transform) =
                    scene.get_component_mut::<LocalTransform>(player_light)
            {
                light_transform.position.x = player_pos.x;
                light_transform.position.y = player_pos.y;
            }
            let follow_speed = 8.0;
            let t = 1.0 - (-follow_speed * dt_s).exp();
            let camera = scene.camera_mut();
            camera.center = camera.center.lerp(player_pos, t);
        }

        self.sky_time_s += dt_s;
        if let Some(sky) = self.sky_entity
            && let Some(transform) = scene.get_component_mut::<LocalTransform>(sky)
        {
            let wave = (self.sky_time_s * 0.08 * std::f32::consts::TAU).sin();
            transform.position.y = wave * 12.0;
        }

        SceneTransition::None
    }
}

#[cfg(test)]
mod tests {
    use super::merge_static_tile_colliders;
    use engine::core::vec2d::Array2D;
    use engine::scene::collisions::{Collider, CollisionShape};
    use glam::Vec2;

    #[test]
    fn merge_static_tile_colliders_keeps_layers_separate() {
        let layers = Array2D::new_init(2, 1, |x, _| if x == 0 { 1 } else { 8 });
        let colliders = merge_static_tile_colliders(&layers, Vec2::splat(16.0));
        assert_eq!(colliders.len(), 2);

        let mut found_layer_1 = false;
        let mut found_layer_8 = false;
        for collider in colliders {
            let Collider::Static(c) = collider else {
                panic!("expected static collider");
            };
            match c.layer {
                1 => found_layer_1 = true,
                8 => found_layer_8 = true,
                other => panic!("unexpected layer {}", other),
            }
            let CollisionShape::Rect { size, .. } = c.shape else {
                panic!("expected rect collider");
            };
            assert!((size.x - 16.0).abs() < 1e-5);
            assert!((size.y - 16.0).abs() < 1e-5);
        }
        assert!(found_layer_1 && found_layer_8);
    }

    #[test]
    fn merge_static_tile_colliders_merges_same_layer_runs() {
        let layers = Array2D::new_init(3, 1, |x, _| if x < 2 { 1 } else { 0 });
        let colliders = merge_static_tile_colliders(&layers, Vec2::splat(16.0));
        assert_eq!(colliders.len(), 1);

        let Collider::Static(collider) = colliders[0] else {
            panic!("expected static collider");
        };
        assert_eq!(collider.layer, 1);
        let CollisionShape::Rect { size, .. } = collider.shape else {
            panic!("expected rect collider");
        };
        assert!((size.x - 32.0).abs() < 1e-5);
        assert!((size.y - 16.0).abs() < 1e-5);
    }
}
