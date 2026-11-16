/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use crate::assets::{Asset, AssetError, AssetLoader};
use compact_str::CompactString;
use glyphon::fontdb;
use serde::Deserialize;
use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

#[derive(Debug)]
pub struct Font {
    #[allow(dead_code)]
    name: CompactString,
    pub(crate) metadata: FontMetadata,
    /// Font family name as recognized by fontdb (and glyphon).
    pub(crate) family_name: String,
}

impl Asset for Font {
    fn type_name() -> CompactString {
        CompactString::from("font")
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Deserialize)]
pub struct FontMetadata {
    pub font_size: f32,
    pub line_height: f32,
}

pub struct FontLoader {
    font_system: Arc<RwLock<glyphon::FontSystem>>,
}

impl FontLoader {
    pub fn new(font_system: Arc<RwLock<glyphon::FontSystem>>) -> FontLoader {
        FontLoader { font_system }
    }
}

impl AssetLoader for FontLoader {
    fn load_asset(&self, name: &str) -> Result<Box<dyn Any>, AssetError> {
        let meta_path = format!("assets/fonts/{name}.json");
        let font_path = format!("assets/fonts/{name}.ttf");
        let requested_font_path = PathBuf::from(&font_path);

        let metadata: FontMetadata =
            serde_json::from_str(&std::fs::read_to_string(meta_path)?).unwrap();
        // let font_data = std::fs::read(font_path).unwrap();

        self.font_system
            .write()
            .unwrap()
            .db_mut()
            .load_font_file(font_path)?;

        let font_system = self.font_system.read().unwrap();
        let mut faces = font_system.db().faces().filter(|face| {
            matches!(
                &face.source,
                fontdb::Source::File(source_path) if source_path == Path::new(&requested_font_path)
            )
        });

        let face = faces.next().ok_or(AssetError::LoadError)?;

        if cfg!(debug_assertions) {
            let other_faces = faces.collect::<Vec<_>>();
            if other_faces.len() > 1 {
                warn!("Found multiple fonts with the same name: {:?}", other_faces);
            }
        }

        let font = Font {
            name: CompactString::from(name),
            metadata,
            family_name: face
                .families
                .first()
                .ok_or(AssetError::LoadError)?
                .0
                .clone(),
        };

        Ok(Box::new(font))
    }
}
