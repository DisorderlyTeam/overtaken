/*
 * Copyright (c) 2026. DisorderlyTeam
 */

pub mod collisions;
mod components;
mod entity;
mod scene_manager;

use crate::context::{RenderCtx, UpdateCtx};
use crate::render_world::{Camera2d, RenderWorld, SceneRenderData, ShadowSegment};
pub use crate::render_world::{DebugDrawable, DebugShape};
use crate::scene::collisions::{Collider, Collision, CollisionShape};
use bitflags::bitflags;
use components::ComponentWorld;
pub use components::{
    AnimatedSpriteComponent, AnimatedSpriteDefinition, AnimationCell, AnimationClipDef, BlendMode,
    ColliderComponent, Component, ComponentTypeFlags, DebugDrawComponent, Facing, GlobalTransform,
    LightSourceComponent, LocalTransform, Material, NavigationComponent, NavigationMesh,
    SpriteComponent, TextAnchor, TextBufferHandle, TextBufferState, TextComponent, TextLayout,
    TextStyle, TilemapComponent, Tileset, VisibilityComponent,
};
pub use entity::EntityHandle;
use glam::{Mat4, Vec2, Vec3, vec2};
pub use scene_manager::*;
use std::collections::HashSet;
use std::f32::consts::TAU;
use std::sync::atomic::{AtomicU32, Ordering};
use tracing::{debug, instrument, trace};

bitflags! {
    #[derive(Debug, Copy, Clone)]
    pub struct SceneFlags: u8 {
        const UPDATE_BELOW = 1 << 0;
        const DRAW_BELOW = 1 << 1;
    }
}

static NEXT_SCENE_ID: AtomicU32 = AtomicU32::new(0);

pub struct Scene {
    scene_id: u32,
    flags: SceneFlags,
    camera: Camera2d,
    components: ComponentWorld,
    lighting_layer_count: u8,
}

#[derive(Debug, Copy, Clone)]
pub struct RaycastOptions {
    pub layer_mask: u32,
    pub include_static: bool,
    pub include_dynamic: bool,
    pub ignore_entity: Option<EntityHandle>,
}

impl Default for RaycastOptions {
    fn default() -> Self {
        Self {
            layer_mask: u32::MAX,
            include_static: true,
            include_dynamic: true,
            ignore_entity: None,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct RaycastHit {
    pub entity: EntityHandle,
    pub point: Vec2,
    pub normal: Vec2,
    pub distance: f32,
    pub layer: u32,
    pub is_static: bool,
}

#[derive(Clone, Copy)]
struct DynamicColliderEntry {
    entity: EntityHandle,
    shape: CollisionShape,
    weight: f32,
    layer: u32,
    collides_with: u32,
}

#[derive(Clone, Copy)]
struct StaticColliderEntry {
    entity: EntityHandle,
    shape: CollisionShape,
    layer: u32,
    collides_with: u32,
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

impl Scene {
    pub fn new() -> Self {
        let scene_id = NEXT_SCENE_ID.fetch_add(1, Ordering::Relaxed);
        debug!("Created scene with ID {}", scene_id);
        Self {
            scene_id,
            flags: SceneFlags::empty(),
            camera: Camera2d::default(),
            components: ComponentWorld::new(),
            lighting_layer_count: 0,
        }
    }

    pub fn set_lighting_layer_count(&mut self, count: u8) {
        self.lighting_layer_count = count;
    }

    pub fn scene_flags(&self) -> SceneFlags {
        self.flags
    }

    pub fn should_update_below(&self) -> bool {
        self.flags.contains(SceneFlags::UPDATE_BELOW)
    }

    pub fn should_draw_below(&self) -> bool {
        self.flags.contains(SceneFlags::DRAW_BELOW)
    }

    pub fn set_scene_flags(&mut self, flags: SceneFlags) {
        self.flags = flags;
    }

    pub fn camera(&self) -> &Camera2d {
        &self.camera
    }

    pub fn camera_mut(&mut self) -> &mut Camera2d {
        &mut self.camera
    }

    pub fn spawn_entity(&mut self) -> EntityHandle {
        self.components.spawn(self.scene_id)
    }

    pub fn spawn_spatial_entity(&mut self, local: LocalTransform) -> EntityHandle {
        let entity = self.components.spawn(self.scene_id);
        let _ = self.components.add_component(entity, local);
        let _ = self
            .components
            .add_component(entity, GlobalTransform::default());
        let _ = self
            .components
            .add_component(entity, VisibilityComponent::default());
        entity
    }

    pub fn spawn_sprite_entity(
        &mut self,
        local: LocalTransform,
        sprite: SpriteComponent,
    ) -> EntityHandle {
        let entity = self.spawn_spatial_entity(local);
        let _ = self.components.add_component(entity, sprite);
        entity
    }

    pub fn spawn_text_entity(
        &mut self,
        local: LocalTransform,
        text: TextComponent,
    ) -> EntityHandle {
        let entity = self.spawn_spatial_entity(local);
        let _ = self.components.add_component(entity, text);
        entity
    }

    pub fn despawn_entity(&mut self, entity: EntityHandle) -> bool {
        self.components.despawn(entity)
    }

    pub fn is_handle_valid(&self, entity: EntityHandle) -> bool {
        self.scene_id == entity.scene_id()
    }
    pub fn is_entity_alive(&self, entity: EntityHandle) -> bool {
        self.is_handle_valid(entity) && self.components.is_alive(entity)
    }

    fn ensure_entity_alive(&self, entity: EntityHandle) {
        if !self.is_entity_alive(entity) {
            panic!("Tried to access dead entity");
        }
    }

    pub fn add_component<T: Component>(&mut self, entity: EntityHandle, component: T) -> bool {
        self.ensure_entity_alive(entity);
        let _ = self.components.add_component(entity, component);
        true
    }

    pub fn get_component<T: Component>(&self, entity: EntityHandle) -> Option<&T> {
        self.ensure_entity_alive(entity);
        self.components.get_component::<T>(entity)
    }

    pub fn has_component<T: Component>(&self, entity: EntityHandle) -> bool {
        self.ensure_entity_alive(entity);
        self.components.has_component::<T>(entity)
    }

    pub fn get_component_mut<T: Component>(&mut self, entity: EntityHandle) -> Option<&mut T> {
        self.ensure_entity_alive(entity);
        self.components.get_component_mut::<T>(entity)
    }

    pub fn remove_component<T: Component>(&mut self, entity: EntityHandle) -> Option<T> {
        self.ensure_entity_alive(entity);
        self.components.remove_component::<T>(entity)
    }

    pub fn set_parent(&mut self, child: EntityHandle, new_parent: Option<EntityHandle>) -> bool {
        self.ensure_entity_alive(child);
        if let Some(new_parent) = new_parent {
            self.ensure_entity_alive(new_parent);
        }
        self.components.set_parent(child, new_parent)
    }

    pub fn entity_name(&self, entity: EntityHandle) -> Option<&str> {
        self.ensure_entity_alive(entity);
        self.components.entity_name(entity)
    }

    pub fn set_entity_name(&mut self, entity: EntityHandle, name: &str) -> bool {
        self.ensure_entity_alive(entity);
        self.components.set_entity_name(entity, name)
    }

    pub fn set_visible(&mut self, entity: EntityHandle, visible: bool) -> bool {
        self.ensure_entity_alive(entity);
        if let Some(visibility) = self
            .components
            .get_component_mut::<VisibilityComponent>(entity)
        {
            visibility.set_visible(visible);
        } else {
            let _ = self
                .components
                .add_component(entity, VisibilityComponent::new(visible));
        }
        true
    }

    pub fn is_visible(&self, entity: EntityHandle) -> bool {
        self.ensure_entity_alive(entity);
        self.components
            .get_component::<VisibilityComponent>(entity)
            .map(VisibilityComponent::is_effectively_visible)
            .unwrap_or(true)
    }

    pub fn find_named_child(&self, root: EntityHandle, name: &str) -> Option<EntityHandle> {
        self.ensure_entity_alive(root);
        self.components.find_named_child(root, name)
    }

    pub fn entities_with_component<T: Component>(&self) -> Vec<EntityHandle> {
        self.components.entities_with_component::<T>()
    }

    pub fn raycast(
        &self,
        origin: Vec2,
        direction: Vec2,
        max_distance: f32,
        options: RaycastOptions,
    ) -> Option<RaycastHit> {
        if max_distance <= 0.0 || direction.length_squared() <= f32::EPSILON {
            return None;
        }
        let direction = direction.normalize();
        let mut closest: Option<RaycastHit> = None;
        let collider_entities = self
            .components
            .entities_with_component::<ColliderComponent>();
        for entity in collider_entities {
            if !self.is_entity_alive(entity) || options.ignore_entity == Some(entity) {
                continue;
            }
            let Some(global) = self.get_component::<GlobalTransform>(entity).copied() else {
                continue;
            };
            let Some(collider_component) = self.get_component::<ColliderComponent>(entity) else {
                continue;
            };
            for collider in collider_component.colliders() {
                let is_static = collider.is_static();
                if (is_static && !options.include_static)
                    || (!is_static && !options.include_dynamic)
                {
                    continue;
                }
                let layer = collider.layer();
                if (layer & options.layer_mask) == 0 {
                    continue;
                }
                let shape = collider.shape().transform(global.object_to_world);
                let Some((distance, point, normal)) =
                    Self::raycast_shape(origin, direction, max_distance, shape)
                else {
                    continue;
                };
                let hit = RaycastHit {
                    entity,
                    point,
                    normal,
                    distance,
                    layer,
                    is_static,
                };
                if closest.is_none_or(|existing| distance < existing.distance) {
                    closest = Some(hit);
                }
            }
        }
        closest
    }

    pub fn raycast_to(&self, from: Vec2, to: Vec2, options: RaycastOptions) -> Option<RaycastHit> {
        let delta = to - from;
        let max_distance = delta.length();
        if max_distance <= f32::EPSILON {
            return None;
        }
        self.raycast(from, delta / max_distance, max_distance, options)
    }

    pub fn entities_collide(&self, a: EntityHandle, b: EntityHandle) -> bool {
        if a == b || !self.is_entity_alive(a) || !self.is_entity_alive(b) {
            return false;
        }

        let Some(a_global) = self.get_component::<GlobalTransform>(a) else {
            return false;
        };
        let Some(b_global) = self.get_component::<GlobalTransform>(b) else {
            return false;
        };
        let Some(a_colliders) = self.get_component::<ColliderComponent>(a) else {
            return false;
        };
        let Some(b_colliders) = self.get_component::<ColliderComponent>(b) else {
            return false;
        };

        for collider_a in a_colliders.colliders() {
            for collider_b in b_colliders.colliders() {
                if !collider_a.can_collide_with(collider_b) {
                    continue;
                }
                let shape_a = collider_a.shape().transform(a_global.object_to_world);
                let shape_b = collider_b.shape().transform(b_global.object_to_world);
                if Collision::between(shape_a, shape_b).is_some() {
                    return true;
                }
            }
        }

        false
    }

    #[instrument(level = "debug", skip_all)]
    pub fn update_transforms(&mut self) {
        let locals = self.components.entities_with_component::<LocalTransform>();
        let mut roots = Vec::new();

        for entity in locals.iter().copied() {
            if !self.is_entity_alive(entity) {
                continue;
            }

            let Some(local) = self.get_component::<LocalTransform>(entity).copied() else {
                continue;
            };
            let parent = self.components.parent_of(entity);
            let is_root = local.ignore_parent_transform
                || parent.is_none_or(|p| {
                    !self.is_entity_alive(p) || self.get_component::<LocalTransform>(p).is_none()
                });
            if is_root {
                roots.push(entity);
            }
        }

        let mut visited = HashSet::with_capacity(locals.len());
        for root in roots {
            self.propagate_transform(root, None, true, &mut visited);
        }
        for entity in locals {
            if !visited.contains(&entity) {
                self.propagate_transform(entity, None, true, &mut visited);
            }
        }
    }

    #[instrument(level = "debug", skip_all)]
    fn propagate_transform(
        &mut self,
        entity: EntityHandle,
        parent_matrix: Option<Mat4>,
        parent_visible: bool,
        visited: &mut HashSet<EntityHandle>,
    ) {
        if !visited.insert(entity) {
            return;
        }

        let Some(local) = self.get_component::<LocalTransform>(entity).copied() else {
            return;
        };

        let local_matrix = local.as_mat4();
        let global_matrix = if local.ignore_parent_transform {
            local_matrix
        } else if let Some(parent) = parent_matrix {
            parent * local_matrix
        } else {
            local_matrix
        };
        let local_visible = self
            .get_component::<VisibilityComponent>(entity)
            .map(VisibilityComponent::visible)
            .unwrap_or(true);
        let effective_visible = parent_visible && local_visible;
        if let Some(visibility) = self.get_component_mut::<VisibilityComponent>(entity) {
            visibility.set_effective_visible(effective_visible);
        }

        let _ = self
            .components
            .add_component(entity, GlobalTransform::new(global_matrix));
        let children = self.components.children_of(entity);
        for child in children {
            if !self.is_entity_alive(child) {
                continue;
            }
            self.propagate_transform(child, Some(global_matrix), effective_visible, visited);
        }
    }

    #[instrument(level = "debug", skip_all)]
    pub fn update(&mut self, ctx: &mut UpdateCtx<'_>) {
        let updatable_storages = self.components.updatable_storage_type_ids();
        for type_id in updatable_storages {
            let Some(mut storage) = self.components.take_storage(type_id) else {
                continue;
            };
            storage.for_all_mut(&mut |entity, component| {
                component.update(self, entity, ctx.delta_time, ctx);
            });
            self.components.put_storage(type_id, storage);
        }

        self.update_transforms();
        self.resolve_collisions();
        self.update_transforms();
    }

    #[instrument(level = "debug", skip_all)]
    pub fn extract_render_data(&self, render_world: &mut RenderWorld, ctx: &mut RenderCtx<'_>) {
        let mut scene_render_data = SceneRenderData::new(
            self.camera,
            render_world.render_resolution,
            *ctx.shown_debug_layers,
            self.lighting_layer_count,
        );

        for storage in self.components.all_storages() {
            if storage.flags().contains(ComponentTypeFlags::NEEDS_DRAW)
                || (storage
                    .flags()
                    .contains(ComponentTypeFlags::NEEDS_DEBUG_DRAW)
                    && *ctx.shown_debug_layers != 0)
            {
                storage.for_all(&mut |entity, component| {
                    if !self.is_visible(entity) {
                        return;
                    }
                    component.extract_render_data(self, entity, &mut scene_render_data, ctx)
                })
            }
        }

        render_world.scene_passes.push(scene_render_data);
    }

    fn visible_tile_ranges(
        tilemap_transform: Mat4,
        world_to_view: Mat4,
        half_width: f32,
        half_height: f32,
        tile_size: Vec2,
        width: usize,
        height: usize,
    ) -> Option<(
        std::ops::RangeInclusive<usize>,
        std::ops::RangeInclusive<usize>,
    )> {
        if width == 0 || height == 0 {
            return None;
        }

        let local_to_view = world_to_view * tilemap_transform;
        if local_to_view.determinant().abs() <= f32::EPSILON {
            return Some((0..=width - 1, 0..=height - 1));
        }
        let view_to_tile = local_to_view.inverse();
        if !view_to_tile.is_finite() {
            return Some((0..=width - 1, 0..=height - 1));
        }

        if tile_size.x.abs() <= f32::EPSILON || tile_size.y.abs() <= f32::EPSILON {
            return Some((0..=width - 1, 0..=height - 1));
        }

        let screen_corners = [
            Vec2::new(-half_width, -half_height),
            Vec2::new(half_width, -half_height),
            Vec2::new(half_width, half_height),
            Vec2::new(-half_width, half_height),
        ];
        let mut local_corners = [Vec2::ZERO; 4];
        for (i, corner) in screen_corners.iter().enumerate() {
            local_corners[i] = view_to_tile
                .transform_point3(Vec3::new(corner.x, corner.y, 0.0))
                .truncate();
        }
        let mut local_min = Vec2::splat(f32::INFINITY);
        let mut local_max = Vec2::splat(f32::NEG_INFINITY);
        for point in &local_corners {
            local_min = local_min.min(*point);
            local_max = local_max.max(*point);
        }

        let half = tile_size * 0.5;
        let x_start = ((local_min.x - half.x) / tile_size.x).ceil() as i32;
        let x_end = ((local_max.x + half.x) / tile_size.x).floor() as i32;
        let y_start = ((local_min.y - half.y) / tile_size.y).ceil() as i32;
        let y_end = ((local_max.y + half.y) / tile_size.y).floor() as i32;

        let x_min_bound = 0;
        let x_max_bound = (width as i32) - 1;
        let y_min_bound = 0;
        let y_max_bound = (height as i32) - 1;

        if x_end < x_min_bound
            || x_start > x_max_bound
            || y_end < y_min_bound
            || y_start > y_max_bound
        {
            return None;
        }

        let x_start = x_start.clamp(x_min_bound, x_max_bound);
        let x_end = x_end.clamp(x_min_bound, x_max_bound);
        let y_start = y_start.clamp(y_min_bound, y_max_bound);
        let y_end = y_end.clamp(y_min_bound, y_max_bound);

        if x_start > x_end || y_start > y_end {
            return None;
        }

        Some((
            (x_start as usize)..=(x_end as usize),
            (y_start as usize)..=(y_end as usize),
        ))
    }

    fn resolve_collisions(&mut self) {
        let collider_entities = self
            .components
            .entities_with_component::<ColliderComponent>();
        let mut dynamic_colliders = Vec::new();
        let mut static_colliders = Vec::new();

        for entity in collider_entities {
            if !self.is_entity_alive(entity) {
                continue;
            }

            let Some(global) = self.get_component::<GlobalTransform>(entity).copied() else {
                continue;
            };
            let Some(collider_component) = self.get_component::<ColliderComponent>(entity) else {
                continue;
            };

            for collider in collider_component.colliders() {
                let shape = collider.shape().transform(global.object_to_world);
                match collider {
                    Collider::Dynamic(dynamic) => dynamic_colliders.push(DynamicColliderEntry {
                        entity,
                        shape,
                        weight: dynamic.weight,
                        layer: dynamic.layer,
                        collides_with: dynamic.collides_with,
                    }),
                    Collider::Static(static_collider) => {
                        static_colliders.push(StaticColliderEntry {
                            entity,
                            shape,
                            layer: static_collider.layer,
                            collides_with: static_collider.collides_with,
                        })
                    }
                    Collider::Trigger(_) => {}
                }
            }
        }

        for i in 0..dynamic_colliders.len() {
            for j in (i + 1)..dynamic_colliders.len() {
                if dynamic_colliders[i].entity == dynamic_colliders[j].entity {
                    continue;
                }
                if (dynamic_colliders[i].collides_with & dynamic_colliders[j].layer) == 0
                    || (dynamic_colliders[j].collides_with & dynamic_colliders[i].layer) == 0
                {
                    continue;
                }

                let Some(collision) =
                    Collision::between(dynamic_colliders[i].shape, dynamic_colliders[j].shape)
                else {
                    continue;
                };

                let separation = collision.normal() * collision.penetration();
                let (a_scale, b_scale) = Self::dynamic_resolution_factors(
                    dynamic_colliders[i].weight,
                    dynamic_colliders[j].weight,
                );
                let a_delta = -separation * a_scale;
                let b_delta = separation * b_scale;
                let entity_a = dynamic_colliders[i].entity;
                let entity_b = dynamic_colliders[j].entity;

                self.translate_entity_in_world(entity_a, a_delta);
                self.translate_entity_in_world(entity_b, b_delta);
                Self::offset_dynamic_shapes(&mut dynamic_colliders, entity_a, a_delta);
                Self::offset_dynamic_shapes(&mut dynamic_colliders, entity_b, b_delta);

                trace!(
                    ?collision,
                    entity_a = ?entity_a,
                    entity_b = ?entity_b,
                    ?a_delta,
                    ?b_delta,
                    "resolved dynamic-dynamic collision"
                );
            }
        }

        for i in 0..dynamic_colliders.len() {
            for static_collider in &static_colliders {
                if dynamic_colliders[i].entity == static_collider.entity {
                    continue;
                }
                if (dynamic_colliders[i].collides_with & static_collider.layer) == 0
                    || (static_collider.collides_with & dynamic_colliders[i].layer) == 0
                {
                    continue;
                }

                let Some(collision) =
                    Collision::between(dynamic_colliders[i].shape, static_collider.shape)
                else {
                    continue;
                };

                let delta = -collision.normal() * collision.penetration();
                let dynamic_entity = dynamic_colliders[i].entity;
                self.translate_entity_in_world(dynamic_entity, delta);
                Self::offset_dynamic_shapes(&mut dynamic_colliders, dynamic_entity, delta);

                trace!(
                    ?collision,
                    dynamic_entity = ?dynamic_entity,
                    static_entity = ?static_collider.entity,
                    ?delta,
                    "resolved dynamic-static collision"
                );
            }
        }
    }

    fn dynamic_resolution_factors(weight_a: f32, weight_b: f32) -> (f32, f32) {
        let inv_a = Self::inverse_weight(weight_a);
        let inv_b = Self::inverse_weight(weight_b);
        let inv_sum = inv_a + inv_b;

        if inv_sum <= f32::EPSILON {
            (0.5, 0.5)
        } else {
            (inv_a / inv_sum, inv_b / inv_sum)
        }
    }

    fn inverse_weight(weight: f32) -> f32 {
        if !weight.is_finite() || weight <= 0.0 {
            0.0
        } else {
            1.0 / weight
        }
    }

    fn offset_dynamic_shapes(
        dynamic_colliders: &mut [DynamicColliderEntry],
        entity: EntityHandle,
        delta: Vec2,
    ) {
        for collider in dynamic_colliders {
            if collider.entity == entity {
                match &mut collider.shape {
                    CollisionShape::Circle { position, .. }
                    | CollisionShape::Rect { position, .. } => {
                        *position += delta;
                    }
                }
            }
        }
    }

    fn translate_entity_in_world(&mut self, entity: EntityHandle, delta: Vec2) {
        if delta.abs_diff_eq(Vec2::ZERO, f32::EPSILON) {
            return;
        }

        let parent_matrix = self
            .components
            .parent_of(entity)
            .and_then(|parent| self.get_component::<GlobalTransform>(parent).copied())
            .map(|global| global.object_to_world);

        let Some(local) = self.get_component_mut::<LocalTransform>(entity) else {
            return;
        };

        let local_delta = if local.ignore_parent_transform {
            delta
        } else if let Some(parent_matrix) = parent_matrix {
            parent_matrix
                .inverse()
                .transform_vector3(delta.extend(0.0))
                .truncate()
        } else {
            delta
        };

        local.position += local_delta.extend(0.0);
    }

    fn raycast_shape(
        origin: Vec2,
        direction: Vec2,
        max_distance: f32,
        shape: CollisionShape,
    ) -> Option<(f32, Vec2, Vec2)> {
        match shape {
            CollisionShape::Circle { position, radius } => {
                let oc = origin - position;
                let a = direction.dot(direction);
                let b = 2.0 * oc.dot(direction);
                let c = oc.dot(oc) - radius * radius;
                let discriminant = b * b - 4.0 * a * c;
                if discriminant < 0.0 {
                    return None;
                }
                let sqrt_d = discriminant.sqrt();
                let inv_2a = 1.0 / (2.0 * a);
                let t0 = (-b - sqrt_d) * inv_2a;
                let t1 = (-b + sqrt_d) * inv_2a;
                let t = [t0, t1]
                    .into_iter()
                    .filter(|t| *t >= 0.0 && *t <= max_distance)
                    .min_by(|a, b| a.total_cmp(b))?;
                let point = origin + direction * t;
                let normal = (point - position).normalize_or_zero();
                Some((t, point, normal))
            }
            CollisionShape::Rect { position, size } => {
                let half = size * 0.5;
                let min = position - half;
                let max = position + half;
                let inv_dir = Vec2::new(
                    if direction.x.abs() > f32::EPSILON {
                        1.0 / direction.x
                    } else {
                        f32::INFINITY
                    },
                    if direction.y.abs() > f32::EPSILON {
                        1.0 / direction.y
                    } else {
                        f32::INFINITY
                    },
                );
                let t1 = (min - origin) * inv_dir;
                let t2 = (max - origin) * inv_dir;
                let tmin = t1.min(t2);
                let tmax = t1.max(t2);
                let near = tmin.x.max(tmin.y);
                let far = tmax.x.min(tmax.y);
                if near > far || far < 0.0 {
                    return None;
                }
                let t = if near >= 0.0 { near } else { far };
                if t < 0.0 || t > max_distance {
                    return None;
                }
                let point = origin + direction * t;
                let bias = 0.001;
                let normal = if (point.x - min.x).abs() <= bias {
                    Vec2::new(-1.0, 0.0)
                } else if (point.x - max.x).abs() <= bias {
                    Vec2::new(1.0, 0.0)
                } else if (point.y - min.y).abs() <= bias {
                    Vec2::new(0.0, -1.0)
                } else {
                    Vec2::new(0.0, 1.0)
                };
                Some((t, point, normal))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::collisions::{Collider, Collision, CollisionShape, DynamicCollider, StaticCollider};
    use super::{ColliderComponent, LocalTransform, Scene};
    use crate::assets::AssetManager;
    use crate::audio::AudioManager;
    use crate::context::UpdateCtx;
    use crate::input::InputManager;
    use glam::{Mat4, Vec2, Vec3};
    use std::time::Duration;

    fn test_update_ctx<'a>(
        assets: &'a mut AssetManager,
        audio: &'a mut AudioManager,
        inputs: &'a mut InputManager,
        shown_debug_layers: &'a mut u32,
    ) -> UpdateCtx<'a> {
        let timescale = Box::leak(Box::new(1.0f32));
        UpdateCtx {
            assets,
            audio,
            inputs,
            delta_time: Duration::ZERO,
            frame_index: 0,
            last_frame_timings: None,
            last_frame_stats: None,
            shown_debug_layers,
            timescale,
            is_top_scene: true,
        }
    }

    fn extract_translation(mat: Mat4) -> Vec2 {
        mat.transform_point3(Vec3::ZERO).truncate()
    }

    #[test]
    fn child_global_transform_is_parent_multiplied() {
        let mut scene = Scene::new();
        let parent = scene.spawn_spatial_entity(LocalTransform {
            position: Vec3::new(10.0, 5.0, 0.0),
            scale: Vec2::ONE,
            rotation: 0.0,
            ignore_parent_transform: false,
            screen_space: false,
        });
        let child = scene.spawn_spatial_entity(LocalTransform {
            position: Vec3::new(2.0, 3.0, 0.0),
            scale: Vec2::ONE,
            rotation: 0.0,
            ignore_parent_transform: false,
            screen_space: false,
        });
        assert!(scene.set_parent(child, Some(parent)));

        scene.update_transforms();
        let global = scene
            .get_component::<super::GlobalTransform>(child)
            .expect("child has global");
        let translation = extract_translation(global.object_to_world);

        assert_eq!(translation, Vec2::new(12.0, 8.0));
    }

    #[test]
    fn ignore_parent_transform_keeps_child_local_space() {
        let mut scene = Scene::new();
        let parent = scene.spawn_spatial_entity(LocalTransform {
            position: Vec3::new(100.0, 40.0, 0.0),
            ..LocalTransform::default()
        });
        let child = scene.spawn_spatial_entity(LocalTransform {
            position: Vec3::new(7.0, 9.0, 0.0),
            ignore_parent_transform: true,
            ..LocalTransform::default()
        });
        assert!(scene.set_parent(child, Some(parent)));

        scene.update_transforms();
        let global = scene
            .get_component::<super::GlobalTransform>(child)
            .expect("child has global");
        let translation = extract_translation(global.object_to_world);

        assert_eq!(translation, Vec2::new(7.0, 9.0));
    }

    #[test]
    fn parent_visibility_hides_children() {
        let mut scene = Scene::new();
        let parent = scene.spawn_spatial_entity(LocalTransform::from_xyz(0.0, 0.0, 0.0));
        let child = scene.spawn_spatial_entity(LocalTransform::from_xyz(5.0, 0.0, 0.0));
        assert!(scene.set_parent(child, Some(parent)));

        scene.update_transforms();
        assert!(scene.is_visible(parent));
        assert!(scene.is_visible(child));

        assert!(scene.set_visible(parent, false));
        scene.update_transforms();
        assert!(!scene.is_visible(parent));
        assert!(!scene.is_visible(child));

        assert!(scene.set_visible(parent, true));
        scene.update_transforms();
        assert!(scene.is_visible(parent));
        assert!(scene.is_visible(child));
    }

    #[test]
    fn reparenting_updates_old_and_new_relationships() {
        let mut scene = Scene::new();
        let p1 = scene.spawn_spatial_entity(LocalTransform::from_xyz(10.0, 0.0, 0.0));
        let p2 = scene.spawn_spatial_entity(LocalTransform::from_xyz(30.0, 0.0, 0.0));
        let child = scene.spawn_spatial_entity(LocalTransform::from_xyz(1.0, 0.0, 0.0));

        assert!(scene.set_parent(child, Some(p1)));
        assert!(scene.set_parent(child, Some(p2)));
        scene.update_transforms();

        let global = scene
            .get_component::<super::GlobalTransform>(child)
            .expect("child has global");
        let translation = extract_translation(global.object_to_world);
        assert_eq!(translation, Vec2::new(31.0, 0.0));
    }

    #[test]
    fn dynamic_and_static_colliders_are_separated() {
        let mut scene = Scene::new();
        let dynamic = scene.spawn_spatial_entity(LocalTransform::from_xyz(0.0, 0.0, 0.0));
        let wall = scene.spawn_spatial_entity(LocalTransform::from_xyz(1.5, 0.0, 0.0));

        scene.add_component(
            dynamic,
            ColliderComponent::new_single(Collider::Dynamic(DynamicCollider::new(
                CollisionShape::Circle {
                    position: Vec2::ZERO,
                    radius: 1.0,
                },
                1.0,
            ))),
        );
        scene.add_component(
            wall,
            ColliderComponent::new_single(Collider::Static(StaticCollider::new(
                CollisionShape::Rect {
                    position: Vec2::ZERO,
                    size: Vec2::splat(2.0),
                },
            ))),
        );

        let mut assets = AssetManager::new();
        let mut audio = AudioManager::new();
        let mut inputs = InputManager::new();
        let mut shown_debug_layers = 0u32;
        let mut ctx = test_update_ctx(
            &mut assets,
            &mut audio,
            &mut inputs,
            &mut shown_debug_layers,
        );
        scene.update(&mut ctx);

        let dynamic_global = scene
            .get_component::<super::GlobalTransform>(dynamic)
            .expect("dynamic has global");
        let wall_global = scene
            .get_component::<super::GlobalTransform>(wall)
            .expect("wall has global");
        let dynamic_shape = CollisionShape::Circle {
            position: Vec2::ZERO,
            radius: 1.0,
        }
        .transform(dynamic_global.object_to_world);
        let wall_shape = CollisionShape::Rect {
            position: Vec2::ZERO,
            size: Vec2::splat(2.0),
        }
        .transform(wall_global.object_to_world);

        assert!(Collision::between(dynamic_shape, wall_shape).is_none());
    }

    #[test]
    fn dynamic_colliders_split_resolution_by_weight() {
        let mut scene = Scene::new();
        let light = scene.spawn_spatial_entity(LocalTransform::from_xyz(0.0, 0.0, 0.0));
        let heavy = scene.spawn_spatial_entity(LocalTransform::from_xyz(1.0, 0.0, 0.0));

        scene.add_component(
            light,
            ColliderComponent::new_single(Collider::Dynamic(DynamicCollider::new(
                CollisionShape::Circle {
                    position: Vec2::ZERO,
                    radius: 1.0,
                },
                1.0,
            ))),
        );
        scene.add_component(
            heavy,
            ColliderComponent::new_single(Collider::Dynamic(DynamicCollider::new(
                CollisionShape::Circle {
                    position: Vec2::ZERO,
                    radius: 1.0,
                },
                3.0,
            ))),
        );

        let mut assets = AssetManager::new();
        let mut audio = AudioManager::new();
        let mut inputs = InputManager::new();
        let mut shown_debug_layers = 0u32;
        let mut ctx = test_update_ctx(
            &mut assets,
            &mut audio,
            &mut inputs,
            &mut shown_debug_layers,
        );
        scene.update(&mut ctx);

        let light_pos = scene
            .get_component::<LocalTransform>(light)
            .expect("light has local")
            .position
            .x;
        let heavy_pos = scene
            .get_component::<LocalTransform>(heavy)
            .expect("heavy has local")
            .position
            .x;

        assert!((light_pos + 0.75).abs() < 1e-4);
        assert!((heavy_pos - 1.25).abs() < 1e-4);
    }

    #[test]
    fn collision_layers_can_prevent_resolution() {
        let mut scene = Scene::new();
        let a = scene.spawn_spatial_entity(LocalTransform::from_xyz(0.0, 0.0, 0.0));
        let b = scene.spawn_spatial_entity(LocalTransform::from_xyz(1.0, 0.0, 0.0));

        let mut a_collider = DynamicCollider::new(
            CollisionShape::Circle {
                position: Vec2::ZERO,
                radius: 1.0,
            },
            1.0,
        );
        a_collider.layer = 1 << 0;
        a_collider.collides_with = 1 << 0;
        scene.add_component(
            a,
            ColliderComponent::new_single(Collider::Dynamic(a_collider)),
        );

        let mut b_collider = DynamicCollider::new(
            CollisionShape::Circle {
                position: Vec2::ZERO,
                radius: 1.0,
            },
            1.0,
        );
        b_collider.layer = 1 << 1;
        b_collider.collides_with = 1 << 1;
        scene.add_component(
            b,
            ColliderComponent::new_single(Collider::Dynamic(b_collider)),
        );

        let mut assets = AssetManager::new();
        let mut audio = AudioManager::new();
        let mut inputs = InputManager::new();
        let mut shown_debug_layers = 0u32;
        let mut ctx = test_update_ctx(
            &mut assets,
            &mut audio,
            &mut inputs,
            &mut shown_debug_layers,
        );
        scene.update(&mut ctx);

        let a_pos = scene
            .get_component::<LocalTransform>(a)
            .expect("a has local")
            .position
            .truncate();
        let b_pos = scene
            .get_component::<LocalTransform>(b)
            .expect("b has local")
            .position
            .truncate();

        assert!((a_pos - Vec2::new(0.0, 0.0)).length() < 1e-5);
        assert!((b_pos - Vec2::new(1.0, 0.0)).length() < 1e-5);
    }
}
