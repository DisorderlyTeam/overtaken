/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use crate::assets::audio::Audio;
use compact_str::CompactString;
use kira::{Decibels, Easing, Tween, Value};
use std::time::Duration;
use tracing::warn;

pub struct AudioManager {
    inner: Option<kira::AudioManager>,
    master_track: Option<kira::track::TrackHandle>,
    music_track: Option<kira::track::TrackHandle>,
    sfx_track: Option<kira::track::TrackHandle>,
    current_music: Option<kira::sound::static_sound::StaticSoundHandle>,
    current_music_name: Option<CompactString>,
    music_tween: Tween,
    master_volume_percent: u8,
    music_volume_percent: u8,
    sfx_volume_percent: u8,
}

impl Default for AudioManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioManager {
    pub fn new() -> Self {
        let music_tween = Tween {
            start_time: kira::StartTime::Immediate,
            duration: Duration::from_secs(3),
            easing: Easing::InOutPowi(2),
        };

        let Ok(mut kira) = kira::AudioManager::new(Default::default()) else {
            warn!("Audio output is unavailable; continuing with a silent audio manager");
            return AudioManager {
                inner: None,
                master_track: None,
                music_track: None,
                sfx_track: None,
                current_music: None,
                current_music_name: None,
                music_tween,
                master_volume_percent: 100,
                music_volume_percent: 100,
                sfx_volume_percent: 100,
            };
        };

        let Ok(mut master_track) = kira.add_sub_track(Default::default()) else {
            warn!("Failed to create audio master track; continuing silently");
            return AudioManager {
                inner: Some(kira),
                master_track: None,
                music_track: None,
                sfx_track: None,
                current_music: None,
                current_music_name: None,
                music_tween,
                master_volume_percent: 100,
                music_volume_percent: 100,
                sfx_volume_percent: 100,
            };
        };

        let music_track = master_track.add_sub_track(Default::default()).ok();
        let sfx_track = master_track.add_sub_track(Default::default()).ok();

        let mut manager = AudioManager {
            inner: Some(kira),
            master_track: Some(master_track),
            music_track,
            sfx_track,
            current_music: None,
            current_music_name: None,
            music_tween,
            master_volume_percent: 100,
            music_volume_percent: 100,
            sfx_volume_percent: 100,
        };
        manager.apply_volumes();
        manager
    }

    fn percent_to_decibels(percent: u8) -> Decibels {
        if percent == 0 {
            Decibels::SILENCE
        } else {
            let amplitude = f32::from(percent) / 100.0;
            Decibels(20.0 * amplitude.log10())
        }
    }

    fn set_track_volume(track: &mut Option<kira::track::TrackHandle>, percent: u8) {
        if let Some(track) = track.as_mut() {
            track.set_volume(
                Value::from(Self::percent_to_decibels(percent)),
                Tween::default(),
            );
        }
    }

    fn apply_volumes(&mut self) {
        Self::set_track_volume(&mut self.master_track, self.master_volume_percent);
        Self::set_track_volume(&mut self.music_track, self.music_volume_percent);
        Self::set_track_volume(&mut self.sfx_track, self.sfx_volume_percent);
    }

    pub fn is_playing_music(&self) -> bool {
        let Some(music) = self.current_music.as_ref() else {
            return false;
        };
        !matches!(
            music.state(),
            kira::sound::PlaybackState::Stopping | kira::sound::PlaybackState::Stopped
        )
    }

    pub fn play_music(&mut self, track: &Audio) {
        debug_assert!(track.is_music());

        if self
            .current_music_name
            .as_ref()
            .is_some_and(|name| name == &track.name)
        {
            return;
        }
        if self.is_playing_music() {
            self.stop_music();
        }

        let Some(music_track) = self.music_track.as_mut() else {
            return;
        };
        let track_name = track.name.clone();
        let music = track.inner.fade_in_tween(self.music_tween);
        self.current_music = Some(music_track.play(music).unwrap());
        self.current_music_name = Some(track_name);
    }

    pub fn master_volume_percent(&self) -> u8 {
        self.master_volume_percent
    }

    pub fn music_volume_percent(&self) -> u8 {
        self.music_volume_percent
    }

    pub fn sfx_volume_percent(&self) -> u8 {
        self.sfx_volume_percent
    }

    pub fn set_master_volume_percent(&mut self, percent: u8) {
        self.master_volume_percent = percent.min(100);
        Self::set_track_volume(&mut self.master_track, self.master_volume_percent);
    }

    pub fn set_music_volume_percent(&mut self, percent: u8) {
        self.music_volume_percent = percent.min(100);
        Self::set_track_volume(&mut self.music_track, self.music_volume_percent);
    }

    pub fn set_sfx_volume_percent(&mut self, percent: u8) {
        self.sfx_volume_percent = percent.min(100);
        Self::set_track_volume(&mut self.sfx_track, self.sfx_volume_percent);
    }

    pub fn stop_music(&mut self) {
        let Some(music) = self.current_music.as_mut() else {
            return;
        };
        music.stop(self.music_tween);
        self.current_music = None;
        self.current_music_name = None;
    }

    pub fn pause_music(&mut self) {
        let Some(music) = self.current_music.as_mut() else {
            return;
        };
        music.pause(self.music_tween);
    }

    pub fn resume_music(&mut self) {
        let Some(music) = self.current_music.as_mut() else {
            return;
        };
        music.resume(self.music_tween);
    }

    pub fn play_sfx(&mut self, track: &Audio) {
        debug_assert!(track.is_sound_effect());

        let Some(sfx_track) = self.sfx_track.as_mut() else {
            return;
        };
        sfx_track.play(track.inner.clone()).unwrap();
    }
}
