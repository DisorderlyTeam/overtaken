/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use super::super::entity::EntityHandle;
use super::{Component, DynComponent};
use bitflags::bitflags;
use std::any::Any;

const SPARSE_NONE: usize = usize::MAX;

bitflags! {
    #[derive(Default, Debug, Copy, Clone)]
    pub struct ComponentTypeFlags: u8 {
        /// The component needs to be updated every frame (e.g., physics, animation).
        ///
        /// [Component::update] will be called every frame.
        const NEEDS_UPDATE = 1 << 0;
        ///
        const NEEDS_DRAW = 1 << 1;
        ///
        const NEEDS_DEBUG_DRAW = 1 << 2;
    }
}

pub(crate) trait DynComponentStorage {
    fn remove_entity(&mut self, entity: EntityHandle);
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn as_any_ref(&self) -> &dyn Any;
    fn flags(&self) -> ComponentTypeFlags;
    fn for_all(&self, callback: &mut dyn FnMut(EntityHandle, &dyn DynComponent));
    fn for_all_mut(&mut self, callback: &mut dyn FnMut(EntityHandle, &mut dyn DynComponent));
}

pub(super) struct ComponentStorage<T: Component> {
    sparse: Vec<usize>,
    dense_entities: Vec<EntityHandle>,
    dense_values: Vec<T>,
    flags: ComponentTypeFlags,
}

impl<T: Component> Default for ComponentStorage<T> {
    fn default() -> Self {
        Self {
            sparse: Vec::new(),
            dense_entities: Vec::new(),
            dense_values: Vec::new(),
            flags: T::flags(),
        }
    }
}

impl<T: Component> ComponentStorage<T> {
    pub(super) fn ensure_sparse_capacity(&mut self, entity_index: usize) {
        if self.sparse.len() <= entity_index {
            self.sparse.resize(entity_index + 1, SPARSE_NONE);
        }
    }

    pub(super) fn dense_index_of(&self, entity: EntityHandle) -> Option<usize> {
        let idx = self.sparse.get(entity.index() as usize).copied()?;
        if idx == SPARSE_NONE {
            return None;
        }
        self.dense_entities
            .get(idx)
            .copied()
            .filter(|stored| *stored == entity)
            .map(|_| idx)
    }

    pub(super) fn insert(&mut self, entity: EntityHandle, value: T) -> Option<T> {
        if let Some(dense_idx) = self.dense_index_of(entity) {
            let old = std::mem::replace(&mut self.dense_values[dense_idx], value);
            return Some(old);
        }

        let dense_idx = self.dense_entities.len();
        self.ensure_sparse_capacity(entity.index() as usize);
        self.sparse[entity.index() as usize] = dense_idx;
        self.dense_entities.push(entity);
        self.dense_values.push(value);
        None
    }

    pub(super) fn get(&self, entity: EntityHandle) -> Option<&T> {
        let dense_idx = self.dense_index_of(entity)?;
        self.dense_values.get(dense_idx)
    }

    pub(super) fn get_mut(&mut self, entity: EntityHandle) -> Option<&mut T> {
        let dense_idx = self.dense_index_of(entity)?;
        self.dense_values.get_mut(dense_idx)
    }

    pub(super) fn remove(&mut self, entity: EntityHandle) -> Option<T> {
        let dense_idx = self.dense_index_of(entity)?;

        let removed_entity = self.dense_entities.swap_remove(dense_idx);
        let removed_value = self.dense_values.swap_remove(dense_idx);
        self.sparse[removed_entity.index() as usize] = SPARSE_NONE;

        if dense_idx < self.dense_entities.len() {
            let moved = self.dense_entities[dense_idx];
            self.sparse[moved.index() as usize] = dense_idx;
        }

        Some(removed_value)
    }

    pub(super) fn entities(&self) -> &[EntityHandle] {
        self.dense_entities.as_slice()
    }
}

impl<T: Component> DynComponentStorage for ComponentStorage<T> {
    fn remove_entity(&mut self, entity: EntityHandle) {
        let _ = self.remove(entity);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn flags(&self) -> ComponentTypeFlags {
        self.flags
    }

    fn for_all(&self, callback: &mut dyn FnMut(EntityHandle, &dyn DynComponent)) {
        for (entity, component) in self.dense_entities.iter().zip(self.dense_values.iter()) {
            callback(*entity, component);
        }
    }

    fn for_all_mut(&mut self, mut callback: &mut dyn FnMut(EntityHandle, &mut dyn DynComponent)) {
        for (entity, component) in self.dense_entities.iter().zip(self.dense_values.iter_mut()) {
            callback(*entity, component);
        }
    }
}
