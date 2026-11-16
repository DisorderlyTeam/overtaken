/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use std::slice;

pub mod arena;
pub mod color;
pub mod math;
pub mod vec2d;

pub(crate) enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    pub fn as_slice(&self) -> &[T] {
        match self {
            OneOrMany::One(value) => slice::from_ref(value),
            OneOrMany::Many(vec) => vec.as_slice(),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            OneOrMany::One(_) => 1,
            OneOrMany::Many(vec) => vec.len(),
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        match self {
            OneOrMany::One(value) => slice::from_mut(value),
            OneOrMany::Many(vec) => vec.as_mut_slice(),
        }
    }
}
