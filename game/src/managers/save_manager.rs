/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use crate::time::GameTime;
use eyre::OptionExt;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use tracing::warn;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct LevelData {
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub best_time: Option<GameTime>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct SaveData {
    levels: HashMap<String, LevelData>,
}

impl SaveData {
    pub fn level_data(&self, level: &str) -> Option<&LevelData> {
        self.levels.get(level)
    }
    pub fn level_data_mut(&mut self, level: String) -> &mut LevelData {
        self.levels.entry(level).or_default()
    }
}

pub struct SaveManager {
    saved_data: SaveData,
}

impl Default for SaveManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SaveManager {
    pub fn new() -> Self {
        let save_path = Self::save_file_path().unwrap();

        Self::ensure_save_dir().unwrap();

        let data = Self::load_save_data(&save_path).unwrap_or_default();

        SaveManager { saved_data: data }
    }

    fn save_file_root() -> eyre::Result<PathBuf> {
        #[cfg(windows)]
        {
            use known_folders::{KnownFolder, get_known_folder_path};
            if let Some(saved_games_dir) = get_known_folder_path(KnownFolder::SavedGames) {
                return Ok(saved_games_dir);
            }
        }

        dirs::data_dir().ok_or_eyre("couldn't get data dir")
    }

    pub fn save_file_path() -> eyre::Result<PathBuf> {
        Ok(Self::save_file_root()?
            .join("Disorderly")
            .join("Overtaken")
            .join("save.json"))
    }

    pub fn ensure_save_dir() -> eyre::Result<()> {
        let save_path = Self::save_file_path()?;
        let save_dir = save_path
            .parent()
            .ok_or_eyre("couldn't get parent of path")?;
        fs::create_dir_all(save_dir)?;
        Ok(())
    }

    fn load_save_data(save_path: &Path) -> Option<SaveData> {
        let Ok(file) = File::open(save_path).inspect_err(|e| match e.kind() {
            ErrorKind::NotFound => warn!("Save file {} not found", save_path.display()),
            _ => panic!("Error when opening save data: {e}"),
        }) else {
            return None;
        };

        serde_json::from_reader(file)
            .inspect_err(|e| {
                panic!("Error when parsing save data: {e}");
            })
            .ok()
    }

    pub fn save_data(&self) -> &SaveData {
        &self.saved_data
    }

    pub fn save_data_mut(&mut self) -> &mut SaveData {
        &mut self.saved_data
    }

    pub fn save(&self) -> eyre::Result<()> {
        Self::ensure_save_dir()?;
        let mut file = File::create(Self::save_file_path()?)?;
        let data = serde_json::to_string(&self.saved_data)?;
        file.write_all(data.as_bytes())?;
        Ok(())
    }
}
