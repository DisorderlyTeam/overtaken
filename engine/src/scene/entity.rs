/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::core::arena::ArenaHandle;
use compact_str::CompactString;
use std::fmt::Debug;

pub struct Entity {
    name: CompactString,
    parent: Option<EntityHandle>,
    children: Vec<EntityHandle>,
}

impl Default for Entity {
    fn default() -> Self {
        Self {
            name: CompactString::new(""),
            parent: None,
            children: Vec::new(),
        }
    }
}

impl Entity {
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn set_name(&mut self, name: impl Into<CompactString>) {
        self.name = name.into();
    }

    pub(crate) fn parent(&self) -> Option<EntityHandle> {
        self.parent
    }

    pub(crate) fn set_parent(&mut self, parent: Option<EntityHandle>) {
        self.parent = parent;
    }

    pub(crate) fn children(&self) -> &[EntityHandle] {
        self.children.as_slice()
    }

    pub(crate) fn add_child(&mut self, child: EntityHandle) {
        if !self.children.contains(&child) {
            self.children.push(child);
        }
    }

    pub(crate) fn remove_child(&mut self, child: EntityHandle) {
        self.children.retain(|&e| e != child);
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct EntityHandle(ArenaHandle<Entity>, u32);

impl EntityHandle {
    pub fn index(&self) -> u32 {
        self.0.index()
    }

    pub fn generation(&self) -> u32 {
        self.0.generation()
    }

    pub(crate) fn from_arena(handle: ArenaHandle<Entity>, scene_id: u32) -> Self {
        Self(handle, scene_id)
    }

    pub(crate) fn into_arena(self) -> ArenaHandle<Entity> {
        self.0
    }

    pub fn scene_id(&self) -> u32 {
        self.1
    }
}

impl Debug for EntityHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "EntityHandle({:?}#{:?})",
            self.0.index(),
            self.0.generation()
        )
    }
}
