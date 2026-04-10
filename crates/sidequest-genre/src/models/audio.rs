//! Audio configuration from `audio.yaml` and voice presets from `voice_presets.yaml`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// audio.yaml
// ═══════════════════════════════════════════════════════════

/// Audio configuration for music, SFX, and voice.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioConfig {
    /// Mood → track list mappings.
    pub mood_tracks: HashMap<String, Vec<MoodTrack>>,
    /// SFX category → file path list.
    pub sfx_library: HashMap<String, Vec<String>>,
    /// Creature type → voice preset. TTS pipeline was removed in PR #388;
    /// content repos dropped this field before the model was updated. Defaults
    /// to an empty map so genre packs without voice presets load cleanly.
    #[serde(default)]
    pub creature_voice_presets: HashMap<String, CreatureVoicePreset>,
    /// Mixer volume settings.
    pub mixer: MixerConfig,
    /// Themed music collections.
    #[serde(default)]
    pub themes: Vec<AudioTheme>,
    /// AI music generation configuration.
    #[serde(default)]
    pub ai_generation: Option<AudioAiGeneration>,
    /// Mood keyword mappings (mood → keyword list).
    #[serde(default)]
    pub mood_keywords: HashMap<String, Vec<String>>,
    /// Mixer defaults (alternative name for mixer in some packs).
    #[serde(default)]
    pub mixer_defaults: Option<MixerConfig>,
    /// Mood alias mappings (custom_mood → core_mood or another alias).
    /// Genre packs declare these in audio.yaml to map genre-specific moods
    /// (e.g. "standoff", "saloon") to core moods ("tension", "calm").
    #[serde(default)]
    pub mood_aliases: HashMap<String, String>,
    /// Faction-specific music themes with trigger conditions.
    #[serde(default)]
    pub faction_themes: Vec<FactionThemeDef>,
}

impl AudioConfig {
    /// Empty config with no tracks, SFX, or presets. Used when genre pack is unavailable.
    pub fn empty() -> Self {
        Self {
            mood_tracks: HashMap::new(),
            sfx_library: HashMap::new(),
            creature_voice_presets: HashMap::new(),
            mixer: MixerConfig {
                music_volume: 0.8,
                sfx_volume: 0.9,
                voice_volume: 1.0,
                duck_music_for_voice: true,
                duck_amount_db: 3.0,
                crossfade_default_ms: 500,
            },
            themes: Vec::new(),
            ai_generation: None,
            mood_keywords: HashMap::new(),
            mixer_defaults: None,
            mood_aliases: HashMap::new(),
            faction_themes: Vec::new(),
        }
    }
}

/// A faction-specific music theme with trigger conditions.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FactionThemeDef {
    /// Faction identifier (e.g. "bosozoku", "lowriders").
    pub faction_id: String,
    /// The track to play when this faction theme is triggered.
    pub track: MoodTrack,
    /// Conditions that trigger this faction theme.
    pub triggers: FactionTriggers,
}

/// Trigger conditions for a faction theme.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FactionTriggers {
    /// Trigger when the player is in a location controlled by this faction.
    #[serde(default)]
    pub location: bool,
    /// Trigger when an NPC of this faction is present in a confrontation.
    #[serde(default)]
    pub npc_present: bool,
    /// Trigger when player reputation with this faction meets or exceeds this threshold.
    #[serde(default)]
    pub reputation_threshold: Option<i32>,
}

/// AI music generation configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioAiGeneration {
    /// Whether AI generation is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Model name (e.g., "musicgen_small").
    #[serde(default)]
    pub model: Option<String>,
    /// Maximum generation time in seconds.
    #[serde(default)]
    pub max_generation_time_s: Option<u32>,
    /// Whether to cache generated audio.
    #[serde(default)]
    pub cache_generated: Option<bool>,
}

/// Default energy level for tracks without an explicit energy field.
fn default_energy() -> f64 {
    0.5
}

/// A single music track.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MoodTrack {
    /// File path.
    pub path: String,
    /// Track title.
    pub title: String,
    /// Beats per minute.
    pub bpm: u32,
    /// Energy level (0.0–1.0) for mood intensity matching.
    #[serde(default = "default_energy")]
    pub energy: f64,
}

/// Voice preset for a creature type.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreatureVoicePreset {
    /// Creature type identifier.
    pub creature_type: String,
    /// Description of the voice.
    pub description: String,
    /// Pitch multiplier.
    pub pitch: f64,
    /// Rate multiplier.
    pub rate: f64,
    /// Audio effects chain.
    #[serde(default)]
    pub effects: Vec<AudioEffect>,
}

/// An audio effect in a processing chain.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioEffect {
    /// Effect type (reverb, lowpass_filter, highpass_filter, compressor).
    #[serde(rename = "type")]
    pub effect_type: String,
    /// Effect parameters (e.g., room_size, cutoff_frequency_hz).
    #[serde(default)]
    pub params: HashMap<String, f64>,
}

/// Mixer volume configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MixerConfig {
    /// Music volume (0.0–1.0).
    pub music_volume: f64,
    /// SFX volume (0.0–1.0).
    pub sfx_volume: f64,
    /// Voice volume (0.0–1.0). Kept for protocol compatibility; TTS pipeline
    /// was removed in PR #388 but content repos stripped this field from
    /// audio.yaml before the model was updated. Defaults to 1.0 so genre
    /// packs without the field load cleanly.
    #[serde(default = "default_voice_volume")]
    pub voice_volume: f64,
    /// Whether to duck music during voice. Also TTS-related legacy; defaults
    /// to `false` since there is no voice stream to duck for anymore.
    #[serde(default)]
    pub duck_music_for_voice: bool,
    /// Ducking amount in decibels.
    pub duck_amount_db: f64,
    /// Default crossfade duration in milliseconds.
    pub crossfade_default_ms: u32,
}

fn default_voice_volume() -> f64 {
    1.0
}

/// A themed music collection with variations.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioTheme {
    /// Theme name.
    pub name: String,
    /// Associated mood.
    pub mood: String,
    /// Base prompt text.
    pub base_prompt: String,
    /// Track variations.
    pub variations: Vec<AudioVariation>,
}

/// A single variation within an audio theme.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioVariation {
    /// Variation type (full, ambient, sparse, overture, resolution, tension_build).
    #[serde(rename = "type")]
    pub variation_type: String,
    /// File path.
    pub path: String,
}

impl AudioVariation {
    /// Convert the string `variation_type` to the typed [`TrackVariation`] enum.
    /// Defaults to [`TrackVariation::Full`] for unrecognized types.
    pub fn as_variation(&self) -> TrackVariation {
        match self.variation_type.as_str() {
            "full" => TrackVariation::Full,
            "overture" => TrackVariation::Overture,
            "ambient" => TrackVariation::Ambient,
            "sparse" => TrackVariation::Sparse,
            "tension_build" => TrackVariation::TensionBuild,
            "resolution" => TrackVariation::Resolution,
            other => {
                tracing::warn!(
                    variation_type = %other,
                    "unrecognized AudioVariation type, defaulting to Full"
                );
                TrackVariation::Full
            }
        }
    }
}

/// Typed track variation — cinematic score cue categories.
///
/// Each variation represents a different energy/pacing role in the soundtrack.
/// The [`MusicDirector`] selects a variation based on narrative context (session
/// start, combat transitions, intensity, drama weight) then picks a track from
/// the genre pack's themed variations for that category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TrackVariation {
    /// Default — peak dramatic moment.
    Full,
    /// First arrival, session start, major scene transition.
    Overture,
    /// Background during dialogue, quiet moments.
    Ambient,
    /// Low-intensity exploration, uncertainty.
    Sparse,
    /// Escalating stakes, approaching danger.
    TensionBuild,
    /// After combat ends, quest completion, winding down.
    Resolution,
}

// ═══════════════════════════════════════════════════════════
// voice_presets.yaml
// ═══════════════════════════════════════════════════════════

/// TTS voice preset configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VoicePresets {
    /// Narrator voice configuration.
    pub narrator: VoiceConfig,
    /// Per-archetype voice configurations.
    #[serde(default)]
    pub characters: HashMap<String, VoiceConfig>,
}

/// A single TTS voice configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VoiceConfig {
    /// Piper ONNX model name.
    pub model: String,
    /// Pitch multiplier.
    pub pitch: f64,
    /// Rate multiplier.
    pub rate: f64,
    /// Audio effects chain.
    #[serde(default)]
    pub effects: Vec<AudioEffect>,
}
