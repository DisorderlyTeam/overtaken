/*
 * Copyright (c) 2026. DisorderlyTeam
 */

mod animated_sprite;
mod collider;
mod debug;
mod light_source;
mod navigation;
mod sprite;
mod storage;
mod text;
mod tiles;
mod transforms;
mod visibility;

use super::entity::{Entity, EntityHandle};
use crate::context::RenderCtx;
use crate::context::UpdateCtx;
use crate::core::arena::Arena;
use crate::render_world::SceneRenderData;
use crate::scene::Scene;
use crate::scene::components::storage::DynComponentStorage;
pub use animated_sprite::{
    AnimatedSpriteComponent, AnimatedSpriteDefinition, AnimationCell, AnimationClipDef, Facing,
};
pub use collider::ColliderComponent;
pub use debug::DebugDrawComponent;
pub use light_source::LightSourceComponent;
pub use navigation::{NavigationComponent, NavigationMesh};
pub use sprite::{BlendMode, Material, SpriteComponent};
use std::any::TypeId;
use std::collections::HashMap;
use std::ops::Deref;
use std::time::Duration;
pub use storage::ComponentTypeFlags;
pub use text::{
    TextAnchor, TextBufferHandle, TextBufferState, TextComponent, TextLayout, TextStyle,
};
pub use tiles::{TilemapComponent, Tileset};
pub use transforms::{GlobalTransform, LocalTransform};
pub use visibility::VisibilityComponent;

pub trait Component: 'static {
    /// Specifies component type flags.
    ///
    /// Should return the same set of flags every time.
    fn flags() -> ComponentTypeFlags {
        ComponentTypeFlags::default()
    }

    /// Performs update on this component.
    ///
    /// Only called when this type's [Self::flags] has [ComponentTypeFlags::NEEDS_UPDATE].
    fn update(
        &mut self,
        _scene: &mut Scene,
        _entity: EntityHandle,
        _dt: Duration,
        _ctx: &mut UpdateCtx<'_>,
    ) {
    }

    /// Extracts render data from this component and adds it to the given render data collector.
    ///
    /// Only called when this type's [Self::flags] either has [ComponentTypeFlags::NEEDS_DRAW] or
    /// has [ComponentTypeFlags::NEEDS_DEBUG_DRAW] and debug draw is enabled.
    fn extract_render_data(
        &self,
        scene: &Scene,
        entity: EntityHandle,
        render_data: &mut SceneRenderData,
        ctx: &mut RenderCtx<'_>,
    ) {
    }
}

pub trait DynComponent: 'static {
    fn update(
        &mut self,
        scene: &mut Scene,
        entity: EntityHandle,
        dt: Duration,
        ctx: &mut UpdateCtx<'_>,
    );
    fn extract_render_data(
        &self,
        scene: &Scene,
        entity: EntityHandle,
        render_data: &mut SceneRenderData,
        ctx: &mut RenderCtx<'_>,
    );
}

impl<T: Component> DynComponent for T {
    fn update(
        &mut self,
        scene: &mut Scene,
        entity: EntityHandle,
        dt: Duration,
        ctx: &mut UpdateCtx<'_>,
    ) {
        self.update(scene, entity, dt, ctx)
    }
    fn extract_render_data(
        &self,
        scene: &Scene,
        entity: EntityHandle,
        render_data: &mut SceneRenderData,
        ctx: &mut RenderCtx<'_>,
    ) {
        self.extract_render_data(scene, entity, render_data, ctx);
    }
}

#[derive(Default)]
pub struct ComponentWorld {
    entities: Arena<Entity>,
    storages: HashMap<TypeId, Box<dyn DynComponentStorage>>,
}

impl ComponentWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn(&mut self, scene_id: u32) -> EntityHandle {
        EntityHandle::from_arena(self.entities.insert(Entity::default()), scene_id)
    }

    pub fn despawn(&mut self, entity: EntityHandle) -> bool {
        if !self.is_alive(entity) {
            return false;
        }

        let old_parent = self.parent_of(entity);
        let old_children = self.children_of(entity);
        if let Some(parent) = old_parent
            && let Some(parent_entity) = self.entities.get_mut(parent.into_arena())
        {
            parent_entity.remove_child(entity);
        }
        for child in old_children {
            if let Some(child_entity) = self.entities.get_mut(child.into_arena()) {
                child_entity.set_parent(old_parent);
            }
            if let Some(parent) = old_parent
                && let Some(parent_entity) = self.entities.get_mut(parent.into_arena())
            {
                parent_entity.add_child(child);
            }
        }

        let _ = self.entities.remove(entity.into_arena());
        for storage in self.storages.values_mut() {
            storage.remove_entity(entity);
        }
        true
    }

    pub fn is_alive(&self, entity: EntityHandle) -> bool {
        self.entities.get(entity.into_arena()).is_some()
    }

    pub fn add_component<T: Component>(&mut self, entity: EntityHandle, component: T) -> Option<T> {
        if !self.is_alive(entity) {
            return None;
        }
        self.storage_mut::<T>().insert(entity, component)
    }

    pub fn remove_component<T: Component>(&mut self, entity: EntityHandle) -> Option<T> {
        self.storage_mut::<T>().remove(entity)
    }

    pub fn get_component<T: Component>(&self, entity: EntityHandle) -> Option<&T> {
        if !self.is_alive(entity) {
            return None;
        }
        self.storage_ref::<T>()?.get(entity)
    }

    pub fn get_component_mut<T: Component>(&mut self, entity: EntityHandle) -> Option<&mut T> {
        if !self.is_alive(entity) {
            return None;
        }
        self.storage_mut::<T>().get_mut(entity)
    }

    pub fn has_component<T: Component>(&self, entity: EntityHandle) -> bool {
        self.get_component::<T>(entity).is_some()
    }

    pub fn entities_with_component<T: Component>(&self) -> Vec<EntityHandle> {
        self.storage_ref::<T>()
            .map(|storage| storage.entities().to_vec())
            .unwrap_or_default()
    }

    pub fn parent_of(&self, entity: EntityHandle) -> Option<EntityHandle> {
        self.entities
            .get(entity.into_arena())
            .and_then(|e| e.parent())
    }

    pub fn children_of(&self, entity: EntityHandle) -> Vec<EntityHandle> {
        self.entities
            .get(entity.into_arena())
            .map(|e| e.children().to_vec())
            .unwrap_or_default()
    }

    pub fn set_parent(&mut self, child: EntityHandle, new_parent: Option<EntityHandle>) -> bool {
        if !self.is_alive(child) {
            return false;
        }

        if let Some(parent) = new_parent {
            if parent == child || !self.is_alive(parent) {
                return false;
            }

            let mut cursor = Some(parent);
            while let Some(current) = cursor {
                if current == child {
                    return false;
                }
                cursor = self.parent_of(current);
            }
        }

        if let Some(old_parent) = self.parent_of(child)
            && let Some(old_parent_entity) = self.entities.get_mut(old_parent.into_arena())
        {
            old_parent_entity.remove_child(child);
        }

        if let Some(parent) = new_parent
            && let Some(parent_entity) = self.entities.get_mut(parent.into_arena())
        {
            parent_entity.add_child(child);
        }

        if let Some(child_entity) = self.entities.get_mut(child.into_arena()) {
            child_entity.set_parent(new_parent);
        }
        true
    }

    pub fn entity_name(&self, entity: EntityHandle) -> Option<&str> {
        self.entities.get(entity.into_arena()).map(|e| e.name())
    }

    pub fn set_entity_name(&mut self, entity: EntityHandle, name: &str) -> bool {
        let Some(entity_ref) = self.entities.get_mut(entity.into_arena()) else {
            return false;
        };
        entity_ref.set_name(name);
        true
    }

    pub fn find_named_child(&self, root: EntityHandle, name: &str) -> Option<EntityHandle> {
        let mut stack = self.children_of(root);
        while let Some(entity) = stack.pop() {
            if self
                .entity_name(entity)
                .is_some_and(|candidate| candidate == name)
            {
                return Some(entity);
            }
            stack.extend(self.children_of(entity));
        }
        None
    }

    fn storage_mut<T: Component>(&mut self) -> &mut storage::ComponentStorage<T> {
        let type_id = TypeId::of::<T>();
        let erased = self
            .storages
            .entry(type_id)
            .or_insert_with(|| Box::new(storage::ComponentStorage::<T>::default()));
        erased
            .as_any_mut()
            .downcast_mut::<storage::ComponentStorage<T>>()
            .expect("storage type mismatch")
    }

    fn storage_ref<T: Component>(&self) -> Option<&storage::ComponentStorage<T>> {
        self.storages.get(&TypeId::of::<T>()).and_then(|erased| {
            erased
                .as_any_ref()
                .downcast_ref::<storage::ComponentStorage<T>>()
        })
    }

    pub(crate) fn all_storages(&self) -> impl Iterator<Item = &dyn DynComponentStorage> {
        self.storages.values().map(|s| s.as_ref())
    }

    pub(crate) fn all_storages_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut dyn DynComponentStorage> {
        self.storages
            .values_mut()
            .map(|s| -> &mut dyn DynComponentStorage { s.as_mut() })
    }

    pub(crate) fn updatable_storage_type_ids(&self) -> Vec<TypeId> {
        self.storages
            .iter()
            .filter_map(|(type_id, storage)| {
                if storage.flags().contains(ComponentTypeFlags::NEEDS_UPDATE) {
                    Some(*type_id)
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn take_storage(&mut self, type_id: TypeId) -> Option<Box<dyn DynComponentStorage>> {
        self.storages.remove(&type_id)
    }

    pub(crate) fn put_storage(&mut self, type_id: TypeId, storage: Box<dyn DynComponentStorage>) {
        self.storages.insert(type_id, storage);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct TestComponent {
        value: i32,
    }

    impl Component for TestComponent {}

    #[test]
    fn add_get_replace_and_remove_component() {
        let mut world = ComponentWorld::new();
        let entity = world.spawn(0);

        assert_eq!(
            world.add_component(entity, TestComponent { value: 1 }),
            None,
        );
        assert_eq!(
            world
                .get_component::<TestComponent>(entity)
                .map(|c| c.value),
            Some(1)
        );
        assert!(world.has_component::<TestComponent>(entity));

        assert_eq!(
            world.add_component(entity, TestComponent { value: 2 }),
            Some(TestComponent { value: 1 }),
        );
        assert_eq!(
            world
                .get_component::<TestComponent>(entity)
                .map(|c| c.value),
            Some(2)
        );

        assert_eq!(
            world.remove_component::<TestComponent>(entity),
            Some(TestComponent { value: 2 })
        );
        assert!(!world.has_component::<TestComponent>(entity));
    }

    #[test]
    fn get_mut_updates_component() {
        let mut world = ComponentWorld::new();
        let entity = world.spawn(0);
        let _ = world.add_component(entity, TestComponent { value: 5 });

        let component = world
            .get_component_mut::<TestComponent>(entity)
            .expect("component exists");
        component.value = 9;

        assert_eq!(
            world
                .get_component::<TestComponent>(entity)
                .map(|c| c.value),
            Some(9)
        );
    }

    #[test]
    fn despawn_clears_components_and_invalidates_entity() {
        let mut world = ComponentWorld::new();
        let entity = world.spawn(0);
        let _ = world.add_component(entity, TestComponent { value: 10 });

        assert!(world.despawn(entity));
        assert!(!world.is_alive(entity));
        assert!(!world.has_component::<TestComponent>(entity));
        assert!(!world.despawn(entity));
    }

    #[test]
    fn stale_entity_add_component_returns_none() {
        let mut world = ComponentWorld::new();
        let entity = world.spawn(0);
        assert!(world.despawn(entity));

        assert_eq!(
            world.add_component(entity, TestComponent { value: 3 }),
            None,
        );
    }

    #[test]
    fn respawn_reuses_index_with_new_generation() {
        let mut world = ComponentWorld::new();
        let first = world.spawn(0);
        assert!(world.despawn(first));
        let second = world.spawn(0);

        assert_eq!(first.index(), second.index());
        assert_ne!(first.generation(), second.generation());
    }

    #[test]
    fn stale_entity_get_returns_none() {
        let mut world = ComponentWorld::new();
        let entity = world.spawn(0);
        assert!(world.despawn(entity));
        assert!(world.get_component::<TestComponent>(entity).is_none());
    }

    #[test]
    fn despawn_reparents_children_to_deleted_entities_parent() {
        let mut world = ComponentWorld::new();
        let grandparent = world.spawn(0);
        let parent = world.spawn(0);
        let child = world.spawn(0);

        assert!(world.set_parent(parent, Some(grandparent)));
        assert!(world.set_parent(child, Some(parent)));
        assert_eq!(world.parent_of(child), Some(parent));

        assert!(world.despawn(parent));

        assert_eq!(world.parent_of(child), Some(grandparent));
        assert!(world.children_of(grandparent).contains(&child));
    }
}
