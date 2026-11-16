/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use crate::assets::{Asset, AssetError, AssetLoader};
use compact_str::CompactString;
use kira::sound::Region;
use std::any::Any;
use tracing::{error, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioKind {
    Music,
    SoundEffect,
}

impl AudioKind {
    pub fn try_from_path(path: &str) -> Option<Self> {
        if path.starts_with("assets/audio/music/") {
            Some(AudioKind::Music)
        } else if path.starts_with("assets/audio/sfx/") {
            Some(AudioKind::SoundEffect)
        } else {
            None
        }
    }
}

pub struct Audio {
    pub name: CompactString,
    pub kind: AudioKind,
    pub(crate) inner: kira::sound::static_sound::StaticSoundData,
}

impl Audio {
    pub fn is_music(&self) -> bool {
        self.kind == AudioKind::Music
    }

    pub fn is_sound_effect(&self) -> bool {
        self.kind == AudioKind::SoundEffect
    }
}

impl Asset for Audio {
    fn type_name() -> CompactString {
        CompactString::from("audio")
    }
}

#[derive(Default)]
pub struct AudioLoader;

impl AudioLoader {
    pub fn new() -> Self {
        Default::default()
    }
}

impl AssetLoader for AudioLoader {
    fn load_asset(&self, name: &str) -> Result<Box<dyn Any>, AssetError> {
        let kind = AudioKind::try_from_path(name).ok_or_else(|| {
            error!(path = name, "Couldn't recognize audio kind from path");
            AssetError::LoadError
        })?;

        let mut data = kira::sound::static_sound::StaticSoundData::from_file(name)
            .map_err(|_| AssetError::LoadError)?;

        // Loop music
        if kind == AudioKind::Music {
            data.settings.loop_region = Some((..).into());
        }

        Ok(Box::new(Audio {
            name: CompactString::from(name),
            kind,
            inner: data,
        }))
    }
}
