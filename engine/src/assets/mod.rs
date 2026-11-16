/*
 * Copyright (c) 2026. DisorderlyTeam
 */

mod asset_manager;
pub mod audio;
pub mod font;
pub mod texture;

pub use asset_manager::*;
use compact_str::CompactString;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

pub trait Asset: 'static {
    /// Returns the name of the asset type.
    /// Only used for debugging purposes.
    fn type_name() -> CompactString;
}

/// Raw type of asset handle.
type RawAssetHandle = u64;

/// Handle to an asset of type [T].
///
/// Created when loading assets with [AssetManager::get_handle].
/// To resolve to the asset itself, use [AssetManager::resolve_handle].
pub struct AssetHandle<T: Asset>(RawAssetHandle, PhantomData<T>);

impl<T: Asset> AssetHandle<T> {
    fn new(id: RawAssetHandle) -> Self {
        AssetHandle(id, PhantomData)
    }
}

impl<T: Asset> Copy for AssetHandle<T> {}

impl<T: Asset> Clone for AssetHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Asset> Debug for AssetHandle<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "AssetHandle<{}>({:?})", T::type_name(), self.0)
    }
}

impl<T: Asset> PartialEq for AssetHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: Asset> Eq for AssetHandle<T> {}

impl<T: Asset> Hash for AssetHandle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}
