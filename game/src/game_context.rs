use crate::managers::config_manager::ConfigManager;
use crate::managers::level_manager::LevelManager;
use crate::managers::locale_manager::LocaleManager;
use crate::managers::save_manager::SaveManager;
use engine::audio::AudioManager;
use engine::input::InputManager;
use std::cell::RefCell;
use std::rc::Rc;

pub type SharedGameContext = Rc<RefCell<GameContext>>;

#[derive(Default)]
pub struct GameContext {
    pub config: ConfigManager,
    pub locales: LocaleManager,
    pub saves: SaveManager,
    pub levels: LevelManager,
}

impl GameContext {
    pub fn shared() -> SharedGameContext {
        Rc::new(RefCell::new(Self::default()))
    }

    pub fn master_volume_percent(&self) -> u8 {
        Self::volume_to_percent(self.config.master_volume())
    }

    pub fn music_volume_percent(&self) -> u8 {
        Self::volume_to_percent(self.config.music_volume())
    }

    pub fn sfx_volume_percent(&self) -> u8 {
        Self::volume_to_percent(self.config.sfx_volume())
    }

    pub fn rumble_enabled(&self) -> bool {
        self.config.rumble_enabled()
    }

    pub fn set_master_volume_percent(&mut self, percent: u8) {
        self.config
            .set_master_volume(Self::percent_to_volume(percent));
    }

    pub fn set_music_volume_percent(&mut self, percent: u8) {
        self.config
            .set_music_volume(Self::percent_to_volume(percent));
    }

    pub fn set_sfx_volume_percent(&mut self, percent: u8) {
        self.config.set_sfx_volume(Self::percent_to_volume(percent));
    }

    pub fn set_rumble_enabled(&mut self, enabled: bool) {
        self.config.set_rumble_enabled(enabled);
    }

    pub fn apply_to_audio_input(&self, audio: &mut AudioManager, inputs: &mut InputManager) {
        audio.set_master_volume_percent(self.master_volume_percent());
        audio.set_music_volume_percent(self.music_volume_percent());
        audio.set_sfx_volume_percent(self.sfx_volume_percent());
        inputs.set_rumble_enabled(self.rumble_enabled());
    }

    pub fn ensure_config_saved(&mut self) {
        self.config.ensure_saved();
    }

    fn volume_to_percent(value: f32) -> u8 {
        let clamped = value.clamp(0.0, 1.0) * 100.0;
        clamped.round().min(100.0) as u8
    }

    fn percent_to_volume(percent: u8) -> f32 {
        let clipped = percent.min(100);
        (clipped as f32) / 100.0
    }

    pub fn unlocked_levels(&self) -> u32 {
        (self
            .levels
            .levels()
            .take_while(|level| {
                self.saves
                    .save_data()
                    .level_data(level)
                    .is_some_and(|data| data.completed)
            })
            .count() as u32
            + 1)
        .min(self.levels.count() as u32)
    }
}
