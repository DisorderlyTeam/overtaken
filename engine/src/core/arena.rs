/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

pub struct Arena<T> {
    objects: Vec<ArenaEntry<T>>,
    free_indices: Vec<u32>,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self {
            objects: Vec::with_capacity(128),
            free_indices: Vec::with_capacity(32),
        }
    }
}

impl<T> Arena<T> {
    pub fn insert(&mut self, object: T) -> ArenaHandle<T> {
        let index = if let Some(index) = self.free_indices.pop() {
            index
        } else {
            self.objects.push(ArenaEntry::default());
            self.objects.len() as u32 - 1
        };
        self.objects[index as usize].object = Some(object);
        ArenaHandle {
            index,
            generation: self.objects[index as usize].generation,
            _phantom: PhantomData,
        }
    }

    fn get_entry(&self, handle: ArenaHandle<T>) -> Option<&ArenaEntry<T>> {
        self.objects
            .get(handle.index as usize)
            .filter(|entry| entry.generation == handle.generation)
    }

    fn get_entry_mut(&mut self, handle: ArenaHandle<T>) -> Option<&mut ArenaEntry<T>> {
        self.objects
            .get_mut(handle.index as usize)
            .filter(|entry| entry.generation == handle.generation)
    }

    pub fn remove(&mut self, handle: ArenaHandle<T>) -> Option<T> {
        if let Some(entry) = self.get_entry_mut(handle) {
            entry.generation += 1;
            let obj = entry.object.take();
            self.free_indices.push(handle.index);
            return obj;
        }
        None
    }

    pub fn get(&self, handle: ArenaHandle<T>) -> Option<&T> {
        self.get_entry(handle)?.object.as_ref()
    }

    pub fn get_mut(&mut self, handle: ArenaHandle<T>) -> Option<&mut T> {
        self.get_entry_mut(handle)?.object.as_mut()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ArenaEntry<T> {
    generation: u32,
    object: Option<T>,
}

impl<T> Default for ArenaEntry<T> {
    fn default() -> Self {
        Self {
            generation: 0,
            object: None,
        }
    }
}

pub struct ArenaHandle<T> {
    index: u32,
    generation: u32,
    _phantom: PhantomData<T>,
}

impl<T> ArenaHandle<T> {
    pub fn index(&self) -> u32 {
        self.index
    }
    pub fn generation(&self) -> u32 {
        self.generation
    }
}

impl<T> Debug for ArenaHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ArenaHandle({:?}#{:?})", self.index, self.generation)
    }
}
impl<T> PartialEq for ArenaHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}
impl<T> Eq for ArenaHandle<T> {}
impl<T> Copy for ArenaHandle<T> {}
impl<T> Clone for ArenaHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Hash for ArenaHandle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::{Arena, ArenaHandle};
    use std::marker::PhantomData;

    #[test]
    fn insert_and_get() {
        let mut arena = Arena::default();
        let handle = arena.insert(123_u32);

        assert_eq!(arena.get(handle), Some(&123));
    }

    #[test]
    fn get_mut() {
        let mut arena = Arena::default();
        let handle = arena.insert(10_i32);

        *arena.get_mut(handle).expect("handle should be valid") = 99;

        assert_eq!(arena.get(handle), Some(&99));
    }

    #[test]
    fn new_generation() {
        let mut arena = Arena::default();
        let old = arena.insert("old");

        assert_eq!(arena.remove(old), Some("old"));
        assert!(arena.get(old).is_none());
        assert!(arena.remove(old).is_none());

        let new = arena.insert("new");

        assert_eq!(old.index, new.index);
        assert_ne!(old.generation, new.generation);
        assert_eq!(arena.get(new), Some(&"new"));
        assert!(arena.get(old).is_none());
    }

    #[test]
    fn invalid_handle() {
        let mut arena = Arena::default();
        let handle = ArenaHandle {
            index: 999,
            generation: 42,
            _phantom: PhantomData::<u32>,
        };

        assert!(arena.get(handle).is_none());
        assert!(arena.remove(handle).is_none());
    }
}
