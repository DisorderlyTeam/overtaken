/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tracing::warn;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Configuration {
    master_volume: f32,
    music_volume: f32,
    sfx_volume: f32,
    rumble_enabled: bool,
    lang_id: String,
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            master_volume: 1.0,
            music_volume: 1.0,
            sfx_volume: 1.0,
            rumble_enabled: true,
            lang_id: "en".into(),
        }
    }
}

/// Manager for game configuration. Holds current config values and updates other components when necessary.
pub struct ConfigManager {
    configuration: Configuration,
    saved_configuration: Configuration,
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigManager {
    fn get_config_file_path() -> Option<PathBuf> {
        dirs::config_dir().map(|mut path| {
            path.join("Disorderly")
                .join("Overtaken")
                .join("config.json")
        })
    }

    fn new() -> Self {
        let config_path = Self::get_config_file_path().unwrap();

        Self::ensure_config_dir(config_path.parent().unwrap());

        let configuration = Self::load_config(&config_path).unwrap_or_default();

        ConfigManager {
            saved_configuration: configuration.clone(),
            configuration,
        }
    }

    fn ensure_config_dir(config_dir: &Path) {
        let result = std::fs::create_dir_all(config_dir);
        if let Err(err) = result {
            warn!("Failed to create config dir: {}", err);
        }
    }

    fn load_config(config_path: &Path) -> Option<Configuration> {
        let Ok(file) = std::fs::File::open(config_path).inspect_err(|e| match e.kind() {
            ErrorKind::NotFound => warn!("Config file {} not found", config_path.display()),
            _ => warn!("Error when opening configuration: {e}"),
        }) else {
            return None;
        };

        serde_json::from_reader(file)
            .inspect_err(|e| {
                warn!("Error when parsing configuration: {e}");
            })
            .ok()
    }

    fn try_save_config(config_path: &Path, config: &Configuration) {
        let Ok(file) = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(config_path)
        else {
            warn!("Failed to open config file for writing");
            return;
        };

        let result = serde_json::to_writer(file, config);

        if let Err(err) = result {
            warn!("Failed to save configuration: {err}");
        }
    }

    pub fn master_volume(&self) -> f32 {
        self.configuration.master_volume
    }

    pub fn set_master_volume(&mut self, master_volume: f32) {
        self.configuration.master_volume = master_volume;
    }

    pub fn music_volume(&self) -> f32 {
        self.configuration.music_volume
    }

    pub fn set_music_volume(&mut self, music_volume: f32) {
        self.configuration.music_volume = music_volume;
    }

    pub fn sfx_volume(&self) -> f32 {
        self.configuration.sfx_volume
    }

    pub fn set_sfx_volume(&mut self, sfx_volume: f32) {
        self.configuration.sfx_volume = sfx_volume;
    }

    pub fn rumble_enabled(&self) -> bool {
        self.configuration.rumble_enabled
    }

    pub fn set_rumble_enabled(&mut self, enabled: bool) {
        self.configuration.rumble_enabled = enabled;
    }

    pub fn lang_id(&self) -> &str {
        &self.configuration.lang_id
    }

    pub fn set_lang_id(&mut self, id: &str) {
        self.configuration.lang_id = id.to_string();
    }

    pub fn ensure_saved(&mut self) {
        if self.saved_configuration != self.configuration {
            Self::try_save_config(&Self::get_config_file_path().unwrap(), &self.configuration);
            self.saved_configuration = self.configuration.clone();
        }
    }
}
