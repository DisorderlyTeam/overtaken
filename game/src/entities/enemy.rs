/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::entities::player::PlayerComponent;
use crate::level::MaterialDef;
use crate::scenes::map_scene::OVERTAKE_RANGE_PX;
use engine::assets::AssetHandle;
use engine::assets::audio::Audio;
use engine::context::{RenderCtx, UpdateCtx};
use engine::core::color::Color;
use engine::core::math::SquaredExt;
use engine::input::RumbleMotor;
use engine::render_world::{DebugDrawableInstance, SceneRenderData, SpriteSpace};
use engine::scene::{
    AnimatedSpriteComponent, ColliderComponent, Component, ComponentTypeFlags, EntityHandle,
    GlobalTransform, LightSourceComponent, LocalTransform, NavigationComponent, RaycastOptions,
    Scene,
};
use glam::{Mat4, Vec2};
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

const ENEMY_VIEWCONE_NAME: &str = "enemy_viewcone";
const NAVIGATION_GROUND_NAME: &str = "navigation_ground";
const NAVIGATION_FLYING_NAME: &str = "navigation_flying";

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum EnemyState {
    Idle,
    Alerted,
    Return,
    Overtaken,
    Stunned,
}

#[derive(Debug, Clone)]
pub enum PatrolStep {
    GoTo(Vec2),
    Wait {
        duration: Duration,
        angle: Option<f32>,
    },
}

pub struct EnemyComponent {
    use_flying_nav: bool,
    tags: Vec<String>,
    overtake_duration_s: f32,
    overtaken_speed: f32,
    state: EnemyState,
    patrol: Vec<PatrolStep>,
    patrol_index: usize,
    wait_remaining_s: f32,
    alerted_lost_s: f32,
    return_target: Option<Vec2>,
    idle_speed: f32,
    alerted_speed: f32,
    last_move_facing: Vec2,
    wall_layer_mask: u32,
    can_open_vents: bool,
    repath_cooldown_s: f32,
    cached_nav_path: Vec<Vec2>,
    debug_nav_path: Vec<Vec2>,
    cached_nav_goal: Option<Vec2>,
    last_seen_player_pos: Vec2,
    stunned_remaining_s: f32,
    stunned_collider: Option<ColliderComponent>,
    frozen: bool,
    alert_sound: Option<AssetHandle<Audio>>,
    possess_sound: Option<AssetHandle<Audio>>,
    leave_sound: Option<AssetHandle<Audio>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EnemyTypeConfig {
    #[serde(default)]
    pub use_flying_nav: bool,
    pub idle_speed: f32,
    pub alerted_speed: f32,
    pub overtake_duration_s: f32,
    pub overtaken_speed: f32,
    #[serde(default)]
    pub ignored_collision_layers_mask: u32,
    #[serde(default)]
    pub animation: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub alert_sound: Option<String>,
    #[serde(default)]
    pub possess_sound: Option<String>,
    #[serde(default)]
    pub leave_sound: Option<String>,
    pub material: MaterialDef,
    pub uv_min: (f32, f32),
    pub uv_max: (f32, f32),
    pub sprite_size: (f32, f32),
}

impl EnemyTypeConfig {
    pub fn collides_with_mask(&self) -> u32 {
        u32::MAX & !self.ignored_collision_layers_mask
    }
}

pub type EnemyTypes = HashMap<String, EnemyTypeConfig>;

impl EnemyComponent {
    fn animation_state_name(&self, player_pos: Vec2, enemy_pos: Vec2) -> &'static str {
        match self.state {
            EnemyState::Idle => {
                if enemy_pos.distance(player_pos) <= OVERTAKE_RANGE_PX {
                    "possessable"
                } else {
                    "default"
                }
            }
            EnemyState::Alerted => "alerted",
            EnemyState::Return => "returning",
            EnemyState::Overtaken => "possessed",
            EnemyState::Stunned => "stunned",
        }
    }

    fn sync_animation(
        &self,
        scene: &mut Scene,
        entity: EntityHandle,
        player_pos: Vec2,
        enemy_pos: Vec2,
        moving: bool,
        facing: Vec2,
        dt: Duration,
    ) {
        let Some(anim) = scene.get_component_mut::<AnimatedSpriteComponent>(entity) else {
            return;
        };
        anim.set_state(self.animation_state_name(player_pos, enemy_pos));
        anim.set_moving(moving);
        anim.set_facing_from_direction(facing);
        anim.advance(dt);
    }

    fn navigation_entity(&self, scene: &Scene) -> Option<EntityHandle> {
        let preferred_name = if self.use_flying_nav {
            NAVIGATION_FLYING_NAME
        } else {
            NAVIGATION_GROUND_NAME
        };
        scene
            .entities_with_component::<NavigationComponent>()
            .into_iter()
            .find(|e| scene.entity_name(*e) == Some(preferred_name))
            .or_else(|| {
                scene
                    .entities_with_component::<NavigationComponent>()
                    .into_iter()
                    .next()
            })
    }

    pub fn new(
        enemy_cfg: &EnemyTypeConfig,
        patrol: Vec<PatrolStep>,
        wall_layer_mask: u32,
        can_open_vents: bool,
        alert_sound: Option<AssetHandle<Audio>>,
        possess_sound: Option<AssetHandle<Audio>>,
        leave_sound: Option<AssetHandle<Audio>>,
    ) -> Self {
        Self {
            use_flying_nav: enemy_cfg.use_flying_nav,
            tags: enemy_cfg.tags.clone(),
            overtake_duration_s: enemy_cfg.overtake_duration_s,
            overtaken_speed: enemy_cfg.overtaken_speed,
            state: EnemyState::Idle,
            patrol,
            patrol_index: 0,
            wait_remaining_s: 0.0,
            alerted_lost_s: 0.0,
            return_target: None,
            idle_speed: enemy_cfg.idle_speed,
            alerted_speed: enemy_cfg.alerted_speed,
            last_move_facing: Vec2::X,
            wall_layer_mask,
            can_open_vents,
            repath_cooldown_s: 0.0,
            cached_nav_path: Vec::new(),
            debug_nav_path: Vec::new(),
            cached_nav_goal: None,
            last_seen_player_pos: Vec2::ZERO,
            stunned_remaining_s: 0.0,
            stunned_collider: None,
            frozen: false,
            alert_sound,
            possess_sound,
            leave_sound,
        }
    }

    pub fn overtake_duration_s(&self) -> f32 {
        self.overtake_duration_s
    }

    pub fn overtaken_speed(&self) -> f32 {
        self.overtaken_speed
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn can_open_vents(&self) -> bool {
        self.can_open_vents
    }

    pub fn possess_sound(&self) -> Option<AssetHandle<Audio>> {
        self.possess_sound
    }

    pub fn leave_sound(&self) -> Option<AssetHandle<Audio>> {
        self.leave_sound
    }

    pub fn state(&self) -> EnemyState {
        self.state
    }

    pub fn can_be_overtaken(&self) -> bool {
        self.state == EnemyState::Idle
    }

    pub fn begin_overtaken(&mut self) {
        self.state = EnemyState::Overtaken;
        self.alerted_lost_s = 0.0;
        self.return_target = None;
        self.wait_remaining_s = 0.0;
        self.cached_nav_goal = None;
        self.cached_nav_path.clear();
        self.debug_nav_path.clear();
        self.repath_cooldown_s = 0.0;
        self.frozen = false;
    }

    pub fn set_frozen(&mut self, frozen: bool) {
        self.frozen = frozen;
    }

    pub fn set_overtaken_facing(&mut self, dir: Vec2) {
        if self.state == EnemyState::Overtaken && dir.length_squared() > 0.0 {
            self.last_move_facing = dir.normalize();
        }
    }

    pub fn enter_stunned(&mut self, collider: Option<ColliderComponent>) {
        self.state = EnemyState::Stunned;
        self.stunned_remaining_s = 5.0;
        self.stunned_collider = collider;
        self.alerted_lost_s = 0.0;
        self.return_target = None;
        self.cached_nav_goal = None;
        self.cached_nav_path.clear();
        self.debug_nav_path.clear();
        self.repath_cooldown_s = 0.0;
    }

    pub fn stop_pursuit(&mut self, current_pos: Vec2) {
        if self.state == EnemyState::Alerted {
            self.state = EnemyState::Return;
            self.return_target = self.closest_patrol_point(current_pos);
            self.alerted_lost_s = 0.0;
            self.cached_nav_goal = None;
            self.cached_nav_path.clear();
            self.debug_nav_path.clear();
            self.repath_cooldown_s = 0.0;
        }
    }

    pub fn tick(
        &mut self,
        scene: &mut Scene,
        enemy_entity: EntityHandle,
        player_pos: Vec2,
        player_detectable: bool,
        dt_s: f32,
        ctx: &mut UpdateCtx<'_>,
    ) {
        if self.frozen {
            return;
        }

        let Some(mut enemy_pos) = scene
            .get_component::<LocalTransform>(enemy_entity)
            .map(|t| t.position.truncate())
        else {
            return;
        };
        let original_enemy_pos = enemy_pos;

        let was_alerted = self.state == EnemyState::Alerted;
        let sees_player = player_detectable
            && !matches!(self.state, EnemyState::Overtaken | EnemyState::Stunned)
            && self.sees_player(scene, enemy_entity, enemy_pos, player_pos);
        if !matches!(self.state, EnemyState::Overtaken | EnemyState::Stunned) {
            if sees_player {
                self.state = EnemyState::Alerted;
                self.alerted_lost_s = 0.0;
                self.last_seen_player_pos = player_pos;
                if !was_alerted {
                    ctx.inputs
                        .request_rumble(RumbleMotor::Both, Duration::from_secs_f32(0.5));
                }
                if !was_alerted && let Some(alert_sound) = self.alert_sound {
                    let alert_sound = ctx.assets.resolve_handle::<Audio>(alert_sound).unwrap();
                    ctx.audio.play_sfx(alert_sound);
                }
            } else if self.state == EnemyState::Alerted {
                self.alerted_lost_s += dt_s;
                if enemy_pos.distance_squared(player_pos) < 16.0.squared() {
                    self.last_seen_player_pos = player_pos;
                }
                if self.alerted_lost_s >= 3.0 {
                    self.state = EnemyState::Return;
                    self.return_target = self.closest_patrol_point(enemy_pos);
                }
            }
        }

        // Preserve prior orientation whenever this frame has no movement.
        let mut facing = self.last_move_facing;
        let mut moved = false;
        match self.state {
            EnemyState::Idle => {
                let before = enemy_pos;
                let (new_pos, dir) = self.step_patrol(enemy_pos, dt_s);
                enemy_pos = new_pos;
                if dir.length_squared() > 0.0 {
                    facing = dir;
                    self.last_move_facing = dir;
                }
                moved = enemy_pos.distance_squared(before) > f32::EPSILON;
                self.debug_nav_path.clear();
            }
            EnemyState::Alerted => {
                if enemy_pos.distance_squared(self.last_seen_player_pos) > 8.0.squared() {
                    let before = enemy_pos;
                    let (new_pos, dir) = self.step_with_nav(
                        scene,
                        enemy_pos,
                        self.last_seen_player_pos,
                        self.alerted_speed,
                        dt_s,
                    );
                    enemy_pos = new_pos;
                    if dir.length_squared() > 0.0 {
                        facing = dir;
                        self.last_move_facing = dir;
                    }
                    moved = enemy_pos.distance_squared(before) > f32::EPSILON;
                }
            }
            EnemyState::Return => {
                let target = self
                    .return_target
                    .or_else(|| self.closest_patrol_point(enemy_pos))
                    .unwrap_or(enemy_pos);
                let to_target = target - enemy_pos;
                if to_target.length_squared() <= 36.0 {
                    enemy_pos = target;
                    self.align_patrol_to_nearest_point(target);
                    self.wait_remaining_s = 0.0;
                    self.state = EnemyState::Idle;
                    self.return_target = None;
                } else {
                    let before = enemy_pos;
                    let (new_pos, dir) =
                        self.step_with_nav(scene, enemy_pos, target, self.idle_speed, dt_s);
                    enemy_pos = new_pos;
                    if dir.length_squared() > 0.0 {
                        facing = dir;
                        self.last_move_facing = dir;
                    }
                    moved = enemy_pos.distance_squared(before) > f32::EPSILON;
                }
            }
            EnemyState::Overtaken => {}
            EnemyState::Stunned => {
                self.debug_nav_path.clear();
                self.stunned_remaining_s = (self.stunned_remaining_s - dt_s).max(0.0);
                if self.stunned_remaining_s <= f32::EPSILON {
                    self.state = EnemyState::Return;
                    self.return_target = self.closest_patrol_point(enemy_pos);
                    if let Some(collider) = self.stunned_collider.take() {
                        scene.add_component(enemy_entity, collider);
                    }
                }
            }
        }

        if !moved {
            moved = enemy_pos.distance_squared(original_enemy_pos) > f32::EPSILON;
        }

        if let Some(transform) = scene.get_component_mut::<LocalTransform>(enemy_entity) {
            transform.position.x = enemy_pos.x;
            transform.position.y = enemy_pos.y;
        }

        self.sync_animation(
            scene,
            enemy_entity,
            player_pos,
            enemy_pos,
            moved,
            facing,
            Duration::from_secs_f32(dt_s),
        );

        if let Some(viewcone) = scene.find_named_child(enemy_entity, ENEMY_VIEWCONE_NAME) {
            if let Some(view_transform) = scene.get_component_mut::<LocalTransform>(viewcone) {
                view_transform.rotation = facing.y.atan2(facing.x);
            }
            if let Some(light) = scene.get_component_mut::<LightSourceComponent>(viewcone) {
                light.direction = Vec2::X;
                light.radius = 100.0;
                light.intensity = match self.state {
                    EnemyState::Alerted => 12.0,
                    EnemyState::Stunned => 0.0,
                    _ => 8.0,
                };
                light.color = match self.state {
                    EnemyState::Idle => Color::from_rgba_u8(255, 255, 0, 255),
                    EnemyState::Alerted if sees_player => Color::from_rgba_u8(255, 0, 0, 255),
                    EnemyState::Alerted => Color::from_rgba_u8(255, 64, 0, 255),
                    EnemyState::Return => Color::from_rgba_u8(255, 255, 255, 255),
                    EnemyState::Overtaken => Color::from_rgba_u8(0, 255, 0, 255),
                    EnemyState::Stunned => Color::from_rgba_u8(0, 0, 0, 0),
                };
            }
        }
    }

    fn step_with_nav(
        &mut self,
        scene: &mut Scene,
        current_pos: Vec2,
        target_pos: Vec2,
        speed: f32,
        dt_s: f32,
    ) -> (Vec2, Vec2) {
        let nav_entity = self.navigation_entity(scene);

        let mut next_target = target_pos;
        let mut has_navigable_target = nav_entity.is_none();
        self.repath_cooldown_s = (self.repath_cooldown_s - dt_s).max(0.0);
        let need_repath = self
            .cached_nav_goal
            .is_none_or(|g| g.distance(target_pos) > 24.0)
            || self.repath_cooldown_s <= 0.0
            || self.cached_nav_path.is_empty();
        if need_repath
            && let Some(nav_entity) = nav_entity
            && let Some(path) = scene
                .get_component::<NavigationComponent>(nav_entity)
                .and_then(|nav| nav.find_path(current_pos, target_pos))
        {
            self.cached_nav_path = path.waypoints.clone();
            self.cached_nav_goal = Some(target_pos);
            self.repath_cooldown_s = 0.18;
        } else if need_repath && nav_entity.is_some() {
            self.cached_nav_path.clear();
            self.cached_nav_goal = None;
        }

        while let Some(first) = self.cached_nav_path.first() {
            if first.distance_squared(current_pos) <= 9.0 {
                let _ = self.cached_nav_path.remove(0);
            } else {
                break;
            }
        }
        if let Some(next) = self.cached_nav_path.first().copied() {
            next_target = next;
            has_navigable_target = true;
        }
        if !self.cached_nav_path.is_empty() {
            self.debug_nav_path = self.cached_nav_path.clone();
        } else if has_navigable_target {
            self.debug_nav_path = vec![next_target];
        } else {
            self.debug_nav_path.clear();
        }

        if !has_navigable_target {
            return (current_pos, Vec2::ZERO);
        }

        let to_next = next_target - current_pos;
        if to_next.length_squared() <= f32::EPSILON {
            return (current_pos, Vec2::ZERO);
        }
        let dir = to_next.normalize();
        let step = speed * dt_s;
        let movement = dir * step.min(to_next.length());
        (current_pos + movement, dir)
    }

    fn sees_player(
        &self,
        scene: &Scene,
        enemy_entity: EntityHandle,
        enemy_pos: Vec2,
        player_pos: Vec2,
    ) -> bool {
        let Some(viewcone) = scene.find_named_child(enemy_entity, ENEMY_VIEWCONE_NAME) else {
            return false;
        };
        let Some(light) = scene.get_component::<LightSourceComponent>(viewcone) else {
            return false;
        };
        let Some(view_global) = scene.get_component::<GlobalTransform>(viewcone) else {
            return false;
        };

        let facing_world = view_global
            .object_to_world
            .transform_vector3(light.direction.extend(0.0))
            .truncate();
        let facing = if facing_world.length_squared() > 1e-10 {
            facing_world.normalize()
        } else {
            Vec2::X
        };
        let to_player = player_pos - enemy_pos;
        let distance = to_player.length();
        if distance > light.radius || distance <= f32::EPSILON {
            return false;
        }

        let dir = to_player / distance;
        let half_angle = light.angle * 0.5;
        let cos_half = half_angle.cos();
        if facing.dot(dir) < cos_half {
            return false;
        }

        let hit = scene.raycast_to(
            enemy_pos,
            player_pos,
            RaycastOptions {
                layer_mask: self.wall_layer_mask,
                include_static: true,
                include_dynamic: true,
                ignore_entity: Some(enemy_entity),
            },
        );
        hit.is_none_or(|h| (h.layer & self.wall_layer_mask) == 0)
    }

    fn current_patrol_target(&self) -> Option<Vec2> {
        let step = self.patrol.get(self.patrol_index)?;
        match step {
            PatrolStep::GoTo(target) => Some(*target),
            PatrolStep::Wait { .. } => None,
        }
    }

    fn closest_patrol_point(&self, from: Vec2) -> Option<Vec2> {
        self.patrol
            .iter()
            .filter_map(|step| match step {
                PatrolStep::GoTo(target) => Some(*target),
                PatrolStep::Wait { .. } => None,
            })
            .min_by(|a, b| {
                a.distance_squared(from)
                    .total_cmp(&b.distance_squared(from))
            })
    }

    fn align_patrol_to_nearest_point(&mut self, at: Vec2) {
        let nearest = self
            .patrol
            .iter()
            .enumerate()
            .filter_map(|(idx, step)| match step {
                PatrolStep::GoTo(target) => Some((idx, target.distance_squared(at))),
                PatrolStep::Wait { .. } => None,
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(idx, _)| idx);
        if let Some(idx) = nearest {
            self.patrol_index = idx;
        }
    }

    fn step_patrol(&mut self, mut enemy_pos: Vec2, dt_s: f32) -> (Vec2, Vec2) {
        if self.patrol.is_empty() {
            return (enemy_pos, Vec2::ZERO);
        }

        let mut remaining = dt_s;
        let mut facing = Vec2::ZERO;
        while remaining > 0.0 {
            let Some(step) = self.patrol.get(self.patrol_index) else {
                self.patrol_index = 0;
                continue;
            };
            match step {
                PatrolStep::GoTo(target) => {
                    let to_target = *target - enemy_pos;
                    let dist = to_target.length();
                    if dist <= 1.0 {
                        self.patrol_index = (self.patrol_index + 1) % self.patrol.len();
                        continue;
                    }
                    let dir = to_target / dist;
                    let max_step = self.idle_speed * remaining;
                    let step_len = max_step.min(dist);
                    enemy_pos += dir * step_len;
                    facing = dir;
                    remaining -= step_len / self.idle_speed;
                    if step_len >= dist {
                        self.patrol_index = (self.patrol_index + 1) % self.patrol.len();
                    } else {
                        break;
                    }
                }
                PatrolStep::Wait { duration, angle } => {
                    if self.wait_remaining_s <= 0.0 {
                        self.wait_remaining_s = duration.as_secs_f32();
                    }
                    facing = angle.map(Vec2::from_angle).unwrap_or(self.last_move_facing);
                    let consumed = remaining.min(self.wait_remaining_s);
                    self.wait_remaining_s -= consumed;
                    remaining -= consumed;
                    if self.wait_remaining_s <= 0.0 {
                        self.patrol_index = (self.patrol_index + 1) % self.patrol.len();
                    } else {
                        break;
                    }
                }
            }
        }
        (enemy_pos, facing)
    }
}

impl Component for EnemyComponent {
    fn flags() -> ComponentTypeFlags {
        ComponentTypeFlags::NEEDS_UPDATE | ComponentTypeFlags::NEEDS_DEBUG_DRAW
    }

    fn update(
        &mut self,
        scene: &mut Scene,
        entity: EntityHandle,
        dt: Duration,
        ctx: &mut UpdateCtx<'_>,
    ) {
        let Some(player) = scene
            .entities_with_component::<LocalTransform>()
            .into_iter()
            .find(|e| scene.entity_name(*e) == Some("player"))
        else {
            return;
        };
        let enemies_paused = scene
            .get_component::<PlayerComponent>(player)
            .is_some_and(PlayerComponent::are_enemies_paused);
        if enemies_paused {
            return;
        }
        let Some(player_pos) = scene
            .get_component::<LocalTransform>(player)
            .map(|transform| transform.position.truncate())
        else {
            return;
        };
        let player_invisible = scene
            .get_component::<PlayerComponent>(player)
            .is_some_and(PlayerComponent::is_invisible);
        let player_detectable = scene
            .get_component::<PlayerComponent>(player)
            .map(PlayerComponent::is_detectable)
            .unwrap_or(true)
            && !player_invisible;
        self.tick(
            scene,
            entity,
            player_pos,
            player_detectable,
            dt.as_secs_f32(),
            ctx,
        );
    }

    fn extract_render_data(
        &self,
        scene: &Scene,
        entity: EntityHandle,
        render_data: &mut SceneRenderData,
        ctx: &mut RenderCtx<'_>,
    ) {
        if *ctx.shown_debug_layers & (1 << 3) == 0 || self.debug_nav_path.is_empty() {
            return;
        }

        let Some(enemy_pos) = scene
            .get_component::<LocalTransform>(entity)
            .map(|transform| transform.position.truncate())
        else {
            return;
        };

        let mut vertices = Vec::with_capacity(self.debug_nav_path.len() * 2);
        let mut from = enemy_pos;
        for to in &self.debug_nav_path {
            vertices.push(from);
            vertices.push(*to);
            from = *to;
        }
        if vertices.is_empty() {
            return;
        }

        render_data.debug_drawables.push(DebugDrawableInstance {
            object_to_world: Mat4::IDENTITY,
            depth: 0.0,
            color: Color::BLUE.to_vec4(),
            vertices,
            layer: 1 << 3,
            space: SpriteSpace::World,
        });
    }
}
