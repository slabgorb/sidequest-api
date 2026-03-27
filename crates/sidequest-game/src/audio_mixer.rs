//! Audio mixer coordination — 3-channel mixing commands with ducking during speech.
//!
//! The mixer tracks state for Music, SFX, and Ambience channels, producing
//! [`AudioCue`] commands that the client applies to its Web Audio graph.
//! Most importantly, it ducks music and SFX when TTS voice is playing.

use std::collections::HashMap;

use crate::music_director::{AudioAction, AudioChannel, AudioCue};

/// Configuration for TTS ducking behaviour.
#[derive(Debug, Clone)]
pub struct DuckConfig {
    /// Music/ambience volume during TTS (0.0–1.0, default 0.15).
    pub duck_volume: f32,
    /// Fade-down duration in milliseconds (default 300).
    pub duck_fade_ms: u32,
    /// Fade-up duration in milliseconds (default 500).
    pub restore_fade_ms: u32,
    /// SFX volume during TTS — higher than music so impacts stay audible (default 0.3).
    pub sfx_duck_volume: f32,
}

impl Default for DuckConfig {
    fn default() -> Self {
        Self {
            duck_volume: 0.15,
            duck_fade_ms: 300,
            restore_fade_ms: 500,
            sfx_duck_volume: 0.3,
        }
    }
}

/// Per-channel volume state.
#[derive(Debug, Clone)]
struct ChannelState {
    /// Currently playing track, if any.
    track_id: Option<String>,
    /// Current volume level.
    volume: f32,
    /// Volume before ducking (for restoration).
    base_volume: f32,
    /// Whether this channel is currently ducked.
    is_ducked: bool,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            track_id: None,
            volume: 0.0,
            base_volume: 0.0,
            is_ducked: false,
        }
    }
}

/// 3-channel audio mixer that coordinates music, SFX, and ambience.
///
/// Produces [`AudioCue`] commands for the client. Does not play audio itself.
pub struct AudioMixer {
    channels: HashMap<AudioChannel, ChannelState>,
    duck_config: DuckConfig,
    is_ducked: bool,
}

impl AudioMixer {
    /// Create a new mixer with the given duck configuration.
    pub fn new(config: DuckConfig) -> Self {
        let mut channels = HashMap::new();
        channels.insert(AudioChannel::Music, ChannelState::default());
        channels.insert(AudioChannel::Sfx, ChannelState::default());
        channels.insert(AudioChannel::Ambience, ChannelState::default());
        Self {
            channels,
            duck_config: config,
            is_ducked: false,
        }
    }

    /// Apply a music director cue to the appropriate channel.
    ///
    /// Updates internal state and passes the cue through for broadcast.
    pub fn apply_cue(&mut self, cue: AudioCue) -> Vec<AudioCue> {
        if let Some(state) = self.channels.get_mut(&cue.channel) {
            state.track_id = cue.track_id.clone();
            state.volume = cue.volume;
            state.base_volume = cue.volume;
        }
        vec![cue]
    }

    /// Duck all active channels for TTS playback.
    ///
    /// Returns duck cues for each active channel. Idempotent: calling twice
    /// while already ducked returns an empty vec.
    pub fn on_tts_start(&mut self) -> Vec<AudioCue> {
        if self.is_ducked {
            return vec![];
        }

        let mut cues = Vec::new();
        let duck_vol = self.duck_config.duck_volume;
        let sfx_duck_vol = self.duck_config.sfx_duck_volume;
        let fade_ms = self.duck_config.duck_fade_ms;

        for (&channel, state) in self.channels.iter_mut() {
            // Only duck channels with an active track
            if state.track_id.is_none() {
                continue;
            }

            state.base_volume = state.volume;
            let target = match channel {
                AudioChannel::Sfx => sfx_duck_vol,
                AudioChannel::Music | AudioChannel::Ambience => duck_vol,
            };
            state.volume = target;
            state.is_ducked = true;

            cues.push(AudioCue {
                channel,
                action: AudioAction::Duck,
                track_id: state.track_id.clone(),
                volume: target,
            });

            // Store fade_ms in the cue for client use — we encode it via
            // a convention: the client reads the duck_fade_ms from the mixer
            // config delivered at session start. The cue itself just says "duck".
            let _ = fade_ms; // Fade duration conveyed via session config, not per-cue
        }

        self.is_ducked = true;
        cues
    }

    /// Restore all ducked channels after TTS finishes.
    ///
    /// Returns restore cues for each ducked channel. Idempotent: calling
    /// while not ducked returns an empty vec.
    pub fn on_tts_end(&mut self) -> Vec<AudioCue> {
        if !self.is_ducked {
            return vec![];
        }

        let mut cues = Vec::new();

        for (&channel, state) in self.channels.iter_mut() {
            if !state.is_ducked {
                continue;
            }

            state.volume = state.base_volume;
            state.is_ducked = false;

            cues.push(AudioCue {
                channel,
                action: AudioAction::Restore,
                track_id: state.track_id.clone(),
                volume: state.base_volume,
            });
        }

        self.is_ducked = false;
        cues
    }

    /// Set an ambience track independently of the music director.
    ///
    /// Returns a FadeIn cue for the Ambience channel.
    pub fn set_ambience(&mut self, track_id: &str, volume: f32) -> AudioCue {
        if let Some(state) = self.channels.get_mut(&AudioChannel::Ambience) {
            state.track_id = Some(track_id.to_string());
            state.volume = volume;
            state.base_volume = volume;
        }
        AudioCue {
            channel: AudioChannel::Ambience,
            action: AudioAction::FadeIn,
            track_id: Some(track_id.to_string()),
            volume,
        }
    }
}

impl std::fmt::Debug for AudioMixer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioMixer")
            .field("is_ducked", &self.is_ducked)
            .field("duck_config", &self.duck_config)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mixer() -> AudioMixer {
        AudioMixer::new(DuckConfig::default())
    }

    fn music_cue(track: &str, volume: f32) -> AudioCue {
        AudioCue {
            channel: AudioChannel::Music,
            action: AudioAction::FadeIn,
            track_id: Some(track.to_string()),
            volume,
        }
    }

    #[test]
    fn three_channels_tracked() {
        let m = mixer();
        assert!(m.channels.contains_key(&AudioChannel::Music));
        assert!(m.channels.contains_key(&AudioChannel::Sfx));
        assert!(m.channels.contains_key(&AudioChannel::Ambience));
    }

    #[test]
    fn apply_cue_passthrough() {
        let mut m = mixer();
        let cue = music_cue("battle.ogg", 0.8);
        let result = m.apply_cue(cue.clone());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].track_id, Some("battle.ogg".to_string()));
        assert_eq!(result[0].volume, 0.8);

        // Internal state updated
        let state = m.channels.get(&AudioChannel::Music).unwrap();
        assert_eq!(state.track_id, Some("battle.ogg".to_string()));
        assert_eq!(state.volume, 0.8);
    }

    #[test]
    fn duck_on_tts_start() {
        let mut m = mixer();
        // Set up active music track
        m.apply_cue(music_cue("battle.ogg", 0.7));

        let cues = m.on_tts_start();
        assert!(!cues.is_empty(), "Should produce duck cues for active channels");

        let music_duck = cues.iter().find(|c| c.channel == AudioChannel::Music);
        assert!(music_duck.is_some());
        let duck = music_duck.unwrap();
        assert_eq!(duck.action, AudioAction::Duck);
        assert!((duck.volume - 0.15).abs() < 0.01, "Music should duck to 0.15");
    }

    #[test]
    fn restore_on_tts_end() {
        let mut m = mixer();
        m.apply_cue(music_cue("battle.ogg", 0.7));
        m.on_tts_start();

        let cues = m.on_tts_end();
        assert!(!cues.is_empty());

        let restore = cues.iter().find(|c| c.channel == AudioChannel::Music).unwrap();
        assert_eq!(restore.action, AudioAction::Restore);
        assert!((restore.volume - 0.7).abs() < 0.01, "Should restore to pre-duck volume");
    }

    #[test]
    fn ambience_separate() {
        let mut m = mixer();
        let cue = m.set_ambience("forest.ogg", 0.4);
        assert_eq!(cue.channel, AudioChannel::Ambience);
        assert_eq!(cue.action, AudioAction::FadeIn);
        assert_eq!(cue.track_id, Some("forest.ogg".to_string()));
    }

    #[test]
    fn duck_config_respected() {
        let config = DuckConfig {
            duck_volume: 0.1,
            sfx_duck_volume: 0.2,
            ..Default::default()
        };
        let mut m = AudioMixer::new(config);

        // Set up music and SFX
        m.apply_cue(music_cue("battle.ogg", 0.8));
        m.apply_cue(AudioCue {
            channel: AudioChannel::Sfx,
            action: AudioAction::Play,
            track_id: Some("clash.ogg".to_string()),
            volume: 0.9,
        });

        let cues = m.on_tts_start();
        let music_duck = cues.iter().find(|c| c.channel == AudioChannel::Music).unwrap();
        let sfx_duck = cues.iter().find(|c| c.channel == AudioChannel::Sfx).unwrap();

        assert!((music_duck.volume - 0.1).abs() < 0.01);
        assert!((sfx_duck.volume - 0.2).abs() < 0.01);
    }

    #[test]
    fn sfx_ducks_higher_than_music() {
        let mut m = mixer();
        m.apply_cue(music_cue("bg.ogg", 0.8));
        m.apply_cue(AudioCue {
            channel: AudioChannel::Sfx,
            action: AudioAction::Play,
            track_id: Some("hit.ogg".to_string()),
            volume: 0.8,
        });

        let cues = m.on_tts_start();
        let music_vol = cues.iter().find(|c| c.channel == AudioChannel::Music).unwrap().volume;
        let sfx_vol = cues.iter().find(|c| c.channel == AudioChannel::Sfx).unwrap().volume;
        assert!(sfx_vol > music_vol, "SFX should duck to higher volume than music");
    }

    #[test]
    fn idempotent_duck() {
        let mut m = mixer();
        m.apply_cue(music_cue("bg.ogg", 0.7));

        let first = m.on_tts_start();
        assert!(!first.is_empty());

        let second = m.on_tts_start();
        assert!(second.is_empty(), "Second duck call should be no-op");
    }

    #[test]
    fn no_track_not_ducked() {
        let mut m = mixer();
        // No tracks set on any channel
        let cues = m.on_tts_start();
        assert!(cues.is_empty(), "Channels with no active track should not be ducked");
    }

    #[test]
    fn cue_output_is_vec() {
        let mut m = mixer();
        m.apply_cue(music_cue("bg.ogg", 0.7));
        let cues: Vec<AudioCue> = m.on_tts_start();
        // Verify it's a Vec<AudioCue> (compile-time check via type annotation)
        assert!(!cues.is_empty());
    }

    #[test]
    fn duck_restore_round_trip() {
        let mut m = mixer();
        m.apply_cue(music_cue("theme.ogg", 0.65));
        m.set_ambience("wind.ogg", 0.3);

        // Duck
        let duck_cues = m.on_tts_start();
        assert_eq!(duck_cues.len(), 2); // Music + Ambience

        // Restore
        let restore_cues = m.on_tts_end();
        assert_eq!(restore_cues.len(), 2);

        // Volumes should match original
        let music_restore = restore_cues.iter().find(|c| c.channel == AudioChannel::Music).unwrap();
        assert!((music_restore.volume - 0.65).abs() < 0.01);

        let amb_restore = restore_cues.iter().find(|c| c.channel == AudioChannel::Ambience).unwrap();
        assert!((amb_restore.volume - 0.3).abs() < 0.01);
    }
}
