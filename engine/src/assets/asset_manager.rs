/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::assets::{Asset, AssetHandle, RawAssetHandle};
use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::fmt::Display;
use std::io::ErrorKind;
use std::ops::Deref;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, instrument, trace};

/// Trait for objects that can load assets.
///
/// The type returned in the box in the [load_asset] method has to be the same
/// as the type provided to the [AssetManager::register_loader] method for the loader to be used.
pub trait AssetLoader: 'static {
    fn load_asset(&self, name: &str) -> Result<Box<dyn Any>, AssetError>;
}

/// Object that stores assets and their handles.
///
/// There's a separate storage for each asset type despite
/// the fact that they all get returned as `Box<dyn Any>` from this.
struct AssetStorage {
    names_to_handles: HashMap<String, RawAssetHandle>,
    handles_to_assets: HashMap<RawAssetHandle, Box<dyn Any>>,
}

/// Asset manager of the engine.
///
/// High-level abstraction to simplify asset management and loading.
pub struct AssetManager {
    loaders: HashMap<TypeId, Box<dyn AssetLoader>>,
    storage: HashMap<TypeId, AssetStorage>,
}

impl Default for AssetManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetManager {
    pub fn new() -> AssetManager {
        AssetManager {
            loaders: HashMap::new(),
            storage: HashMap::new(),
        }
    }

    /// Registers an asset loader for a specific type of asset.
    pub fn register_loader<TAsset: Asset, TLoader: AssetLoader>(&mut self, loader: TLoader) {
        debug!(
            loader = type_name::<TLoader>(),
            asset_type = type_name::<TAsset>(),
            "Registering asset loader"
        );
        self.loaders
            .insert(TypeId::of::<TAsset>(), Box::new(loader));
    }

    /// Gets the used asset loader for a specific type of asset.
    ///
    /// If not found, returns `None`.
    fn get_loader<T: Asset>(&self) -> Option<&dyn AssetLoader> {
        self.loaders.get(&TypeId::of::<T>()).map(|v| &**v)
    }

    /// Low-level function that loads the asset using the asset type's loader.
    ///
    /// It always calls the loader. If there's no loader for the asset type, it returns `None`.
    fn try_load_asset<T: Asset>(&self, name: &str) -> Result<Box<dyn Any>, AssetError> {
        let Some(loader) = self.get_loader::<T>() else {
            error!(
                asset_type = type_name::<T>(),
                "No loader found for asset type"
            );
            return Err(AssetError::NoLoader);
        };

        match loader.load_asset(name) {
            Err(AssetError::Io(ErrorKind::NotFound)) | Err(AssetError::NotFound(_)) => {
                Err(AssetError::NotFound(name.to_string()))
            }
            other => other,
        }
    }

    /// Tries finding an existing handle to an asset within our storage.
    ///
    /// Otherwise, if not loaded, returns `None`.
    fn try_get_existing_handle<T: Asset>(&self, name: &str) -> Option<AssetHandle<T>> {
        let storage = self.storage.get(&TypeId::of::<T>())?;

        if let Some(handle) = storage.names_to_handles.get(name) {
            return Some(AssetHandle::new(*handle));
        }

        None
    }

    /// Gets a handle to an asset of type [T] with the given name.
    ///
    /// It may load the asset from a file using the asset type's loader or it
    /// may return an existing asset handle.
    #[instrument(level = "debug", skip(self))]
    pub fn get_handle<T: Asset>(&mut self, name: &str) -> Result<AssetHandle<T>, AssetError> {
        if let Some(handle) = self.try_get_existing_handle::<T>(name) {
            trace!(name, "Found existing asset handle for asset");
            return Ok(handle);
        }

        let asset = self.try_load_asset::<T>(name)?;

        debug!(name, asset_type = type_name::<T>(), "Loaded asset");

        let storage = self
            .storage
            .entry(TypeId::of::<T>())
            .or_insert_with(|| AssetStorage {
                names_to_handles: HashMap::new(),
                handles_to_assets: HashMap::new(),
            });

        let handle = storage.handles_to_assets.len() as RawAssetHandle;
        storage.names_to_handles.insert(name.to_string(), handle);
        storage.handles_to_assets.insert(handle, asset);
        Ok(AssetHandle::new(handle))
    }

    /// Resolves an asset handle to the asset reference.
    ///
    /// If the handle doesn't point to an existing asset, returns `None`.
    ///
    /// It may panic if the stored asset is of a different type than expected.
    #[instrument(level = "trace", skip(self))]
    pub fn resolve_handle<T: Asset>(&self, handle: AssetHandle<T>) -> Option<&T> {
        if let Some(storage) = self.storage.get(&TypeId::of::<T>())
            && let Some(asset) = storage.handles_to_assets.get(&handle.0)
        {
            let asset = asset.deref();
            return Some(asset.downcast_ref::<T>().unwrap_or_else(|| {
                error!(
                    ?handle,
                    expected_type = ?TypeId::of::<&T>(),
                    got_type = ?asset.type_id(),
                    "Asset handle points to an asset of wrong type",
                );
                panic!("Failed to resolve asset handle");
            }));
        }
        None
    }

    /// Clears the asset manager, removing all assets and loaders.
    ///
    /// Used when shutting down the engine.
    pub fn clear(&mut self) {
        self.storage.clear();
        self.loaders.clear();
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AssetError {
    #[error("no suitable loader found for asset type")]
    NoLoader,
    #[error("asset not found: {0}")]
    NotFound(String),
    #[error("asset couldn't be loaded")]
    LoadError,
    #[error("IO error: {0:?}")]
    Io(std::io::ErrorKind),
}

impl From<std::io::Error> for AssetError {
    fn from(e: std::io::Error) -> Self {
        AssetError::Io(e.kind())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use compact_str::CompactString;
    use test_log::test;
    use tracing::info;

    struct TestAsset;

    impl Asset for TestAsset {
        fn type_name() -> CompactString {
            CompactString::from("test_asset")
        }
    }

    struct NotExistingAsset;

    impl Asset for NotExistingAsset {
        fn type_name() -> CompactString {
            CompactString::from("not_existing_asset")
        }
    }

    struct TestAssetLoader;

    impl AssetLoader for TestAssetLoader {
        fn load_asset(&self, _name: &str) -> Result<Box<dyn Any + 'static>, AssetError> {
            Ok(Box::new(TestAsset))
        }
    }

    #[test]
    fn test_load_asset() {
        let mut asset_manager = AssetManager::new();

        asset_manager.register_loader::<TestAsset, _>(TestAssetLoader);

        let handle = asset_manager.get_handle::<TestAsset>("test_asset");
        info!("Asset handle: {:?}", handle);
        assert!(handle.is_ok());
        assert_eq!(
            asset_manager.get_handle::<NotExistingAsset>("not_existing_asset"),
            Err(AssetError::NoLoader)
        );
    }
}
