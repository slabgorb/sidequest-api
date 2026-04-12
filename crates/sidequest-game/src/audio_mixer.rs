//! Audio mixer coordination — 3-channel mixing for Music, SFX, and Ambience.
//!
//! The mixer tracks state for each channel, producing [`AudioCue`] commands
//! that the client applies to its Web Audio graph.

use std::collections::HashMap;

use crate::music_director::{AudioAction, AudioChannel, AudioCue};

/// Per-channel volume state.
#[derive(Debug, Clone)]
struct ChannelState {
    /// Currently playing track, if any.
    track_id: Option<String>,
    /// Current volume level.
    volume: f32,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            track_id: None,
            volume: 0.0,
        }
    }
}

/// 3-channel audio mixer that coordinates music, SFX, and ambience.
///
/// Produces [`AudioCue`] commands for the client. Does not play audio itself.
pub struct AudioMixer {
    channels: HashMap<AudioChannel, ChannelState>,
}

impl Default for AudioMixer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioMixer {
    /// Create a new mixer.
    pub fn new() -> Self {
        let mut channels = HashMap::new();
        channels.insert(AudioChannel::Music, ChannelState::default());
        channels.insert(AudioChannel::Sfx, ChannelState::default());
        channels.insert(AudioChannel::Ambience, ChannelState::default());
        Self { channels }
    }

    /// Apply a music director cue to the appropriate channel.
    ///
    /// Updates internal state and passes the cue through for broadcast.
    pub fn apply_cue(&mut self, cue: AudioCue) -> Vec<AudioCue> {
        if let Some(state) = self.channels.get_mut(&cue.channel) {
            state.track_id = cue.track_id.clone();
            state.volume = cue.volume;
        }
        vec![cue]
    }

    /// Set an ambience track independently of the music director.
    ///
    /// Returns a FadeIn cue for the Ambience channel.
    pub fn set_ambience(&mut self, track_id: &str, volume: f32) -> AudioCue {
        if let Some(state) = self.channels.get_mut(&AudioChannel::Ambience) {
            state.track_id = Some(track_id.to_string());
            state.volume = volume;
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
        f.debug_struct("AudioMixer").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mixer() -> AudioMixer {
        AudioMixer::new()
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
    fn ambience_separate() {
        let mut m = mixer();
        let cue = m.set_ambience("forest.ogg", 0.4);
        assert_eq!(cue.channel, AudioChannel::Ambience);
        assert_eq!(cue.action, AudioAction::FadeIn);
        assert_eq!(cue.track_id, Some("forest.ogg".to_string()));
    }
}
