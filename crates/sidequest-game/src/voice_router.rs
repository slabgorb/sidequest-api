//! Voice routing — map speaker identity to TTS voice parameters.
//!
//! Story 4-6: Resolves character/NPC/narrator names to voice presets
//! from the genre pack config, producing parameters for the daemon TTS endpoint.

use sidequest_genre::{AudioConfig, AudioEffect, CreatureVoicePreset, VoiceConfig, VoicePresets};
use std::collections::HashMap;

/// Default Piper model used when no voice preset is configured.
const DEFAULT_PIPER_MODEL: &str = "en_US-lessac-medium";

/// Default TTS engine name.
const DEFAULT_ENGINE: &str = "piper";

/// A resolved voice assignment ready for TTS synthesis.
#[derive(Debug, Clone)]
pub struct VoiceAssignment {
    /// TTS engine ("piper" or "kokoro").
    pub engine: String,
    /// Voice ID within the engine (e.g., "en_US-lessac-high").
    pub voice_id: String,
    /// Speech speed multiplier (1.0 = normal).
    pub speed: f32,
    /// Pitch multiplier (1.0 = normal).
    pub pitch: f64,
    /// Audio effects chain for post-processing.
    pub effects: Vec<AudioEffect>,
}

impl VoiceAssignment {
    /// Build from a VoiceConfig (from voice_presets.yaml).
    fn from_voice_config(config: &VoiceConfig) -> Self {
        Self {
            engine: infer_engine(&config.model),
            voice_id: config.model.clone(),
            speed: config.rate as f32,
            pitch: config.pitch,
            effects: config.effects.clone(),
        }
    }

    /// Build from a CreatureVoicePreset (from audio.yaml).
    /// Creature presets lack a model name, so we use the default.
    fn from_creature_preset(preset: &CreatureVoicePreset) -> Self {
        Self {
            engine: DEFAULT_ENGINE.to_string(),
            voice_id: DEFAULT_PIPER_MODEL.to_string(),
            speed: preset.rate as f32,
            pitch: preset.pitch,
            effects: preset.effects.clone(),
        }
    }
}

/// Routes speaker identities to voice assignments using genre pack config.
#[derive(Debug, Clone)]
pub struct VoiceRouter {
    /// Narrator voice (always present).
    narrator: VoiceAssignment,
    /// Character archetype → voice assignment.
    characters: HashMap<String, VoiceAssignment>,
    /// Creature type → voice assignment.
    creatures: HashMap<String, VoiceAssignment>,
}

impl VoiceRouter {
    /// Create a router from genre pack voice configuration.
    ///
    /// Uses `voice_presets` for narrator and character archetypes,
    /// and `audio.creature_voice_presets` for creature types.
    /// Falls back to a sensible default narrator if `voice_presets` is None.
    pub fn new(voice_presets: Option<&VoicePresets>, audio: &AudioConfig) -> Self {
        let narrator = voice_presets
            .map(|vp| VoiceAssignment::from_voice_config(&vp.narrator))
            .unwrap_or_else(default_narrator);

        let characters = voice_presets
            .map(|vp| {
                vp.characters
                    .iter()
                    .map(|(name, config)| {
                        (normalize_speaker(name), VoiceAssignment::from_voice_config(config))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let creatures = audio
            .creature_voice_presets
            .iter()
            .map(|(name, preset)| {
                (normalize_speaker(name), VoiceAssignment::from_creature_preset(preset))
            })
            .collect();

        Self { narrator, characters, creatures }
    }

    /// Resolve a speaker name to a voice assignment.
    ///
    /// Lookup order:
    /// 1. "narrator" → narrator preset
    /// 2. Character archetype match (case-insensitive)
    /// 3. Creature type match (case-insensitive)
    /// 4. Fallback → narrator preset
    pub fn route(&self, speaker: &str) -> VoiceAssignment {
        let key = normalize_speaker(speaker);

        if key == "narrator" {
            return self.narrator.clone();
        }

        if let Some(assignment) = self.characters.get(&key) {
            return assignment.clone();
        }

        if let Some(assignment) = self.creatures.get(&key) {
            return assignment.clone();
        }

        // Unknown speaker falls back to narrator voice.
        self.narrator.clone()
    }

    /// Check whether a speaker has an explicit voice assignment
    /// (not falling back to narrator).
    pub fn has_explicit_voice(&self, speaker: &str) -> bool {
        let key = normalize_speaker(speaker);
        key == "narrator" || self.characters.contains_key(&key) || self.creatures.contains_key(&key)
    }

    /// Number of configured voices (narrator + characters + creatures).
    pub fn voice_count(&self) -> usize {
        1 + self.characters.len() + self.creatures.len()
    }
}

/// Infer the TTS engine from a model name.
/// Kokoro models use the pattern "en_male_*" or "en_female_*".
/// Everything else is assumed to be Piper.
fn infer_engine(model: &str) -> String {
    if model.starts_with("en_male") || model.starts_with("en_female") {
        "kokoro".to_string()
    } else {
        DEFAULT_ENGINE.to_string()
    }
}

/// Default narrator when no voice_presets.yaml exists.
fn default_narrator() -> VoiceAssignment {
    VoiceAssignment {
        engine: DEFAULT_ENGINE.to_string(),
        voice_id: DEFAULT_PIPER_MODEL.to_string(),
        speed: 1.0,
        pitch: 1.0,
        effects: Vec::new(),
    }
}

/// Normalize speaker names for case-insensitive lookup.
fn normalize_speaker(name: &str) -> String {
    name.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_voice_config(model: &str, pitch: f64, rate: f64) -> VoiceConfig {
        VoiceConfig {
            model: model.to_string(),
            pitch,
            rate,
            effects: Vec::new(),
        }
    }

    fn test_creature_preset(creature_type: &str, pitch: f64, rate: f64) -> CreatureVoicePreset {
        CreatureVoicePreset {
            creature_type: creature_type.to_string(),
            description: format!("{creature_type} voice"),
            pitch,
            rate,
            effects: vec![AudioEffect {
                effect_type: "reverb".to_string(),
                params: HashMap::from([("room_size".to_string(), 0.8)]),
            }],
        }
    }

    fn minimal_audio_config() -> AudioConfig {
        AudioConfig {
            mood_tracks: HashMap::new(),
            sfx_library: HashMap::new(),
            creature_voice_presets: HashMap::new(),
            mixer: sidequest_genre::MixerConfig {
                music_volume: 0.7,
                sfx_volume: 0.8,
                voice_volume: 1.0,
                duck_music_for_voice: true,
                duck_amount_db: -12.0,
                crossfade_default_ms: 500,
            },
            themes: Vec::new(),
            ai_generation: None,
            mood_keywords: HashMap::new(),
            mixer_defaults: None,
            mood_aliases: HashMap::new(),
        }
    }

    #[test]
    fn narrator_from_voice_presets() {
        let presets = VoicePresets {
            narrator: test_voice_config("en_US-ryan-high", 0.85, 0.95),
            characters: HashMap::new(),
        };
        let router = VoiceRouter::new(Some(&presets), &minimal_audio_config());
        let assignment = router.route("narrator");

        assert_eq!(assignment.engine, "piper");
        assert_eq!(assignment.voice_id, "en_US-ryan-high");
        assert!((assignment.speed - 0.95).abs() < f32::EPSILON);
        assert!((assignment.pitch - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn narrator_fallback_without_presets() {
        let router = VoiceRouter::new(None, &minimal_audio_config());
        let assignment = router.route("narrator");

        assert_eq!(assignment.engine, "piper");
        assert_eq!(assignment.voice_id, DEFAULT_PIPER_MODEL);
        assert!((assignment.speed - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn character_lookup_case_insensitive() {
        let presets = VoicePresets {
            narrator: test_voice_config("en_US-lessac-high", 1.0, 1.0),
            characters: HashMap::from([
                ("Grizzled Veteran".to_string(), test_voice_config("en_US-ryan-high", 0.85, 0.9)),
            ]),
        };
        let router = VoiceRouter::new(Some(&presets), &minimal_audio_config());

        let assignment = router.route("grizzled veteran");
        assert_eq!(assignment.voice_id, "en_US-ryan-high");
        assert!((assignment.speed - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn creature_lookup() {
        let mut audio = minimal_audio_config();
        audio.creature_voice_presets.insert(
            "dragon".to_string(),
            test_creature_preset("dragon", 0.6, 0.7),
        );

        let router = VoiceRouter::new(None, &audio);
        let assignment = router.route("Dragon");

        assert_eq!(assignment.engine, "piper");
        assert_eq!(assignment.voice_id, DEFAULT_PIPER_MODEL);
        assert!((assignment.speed - 0.7).abs() < f32::EPSILON);
        assert!((assignment.pitch - 0.6).abs() < f64::EPSILON);
        assert_eq!(assignment.effects.len(), 1);
        assert_eq!(assignment.effects[0].effect_type, "reverb");
    }

    #[test]
    fn character_takes_priority_over_creature() {
        let presets = VoicePresets {
            narrator: test_voice_config("en_US-lessac-high", 1.0, 1.0),
            characters: HashMap::from([
                ("goblin".to_string(), test_voice_config("en_US-arctic-medium", 1.5, 1.3)),
            ]),
        };
        let mut audio = minimal_audio_config();
        audio.creature_voice_presets.insert(
            "goblin".to_string(),
            test_creature_preset("goblin", 1.2, 1.1),
        );

        let router = VoiceRouter::new(Some(&presets), &audio);
        let assignment = router.route("Goblin");

        // Character preset wins over creature preset.
        assert_eq!(assignment.voice_id, "en_US-arctic-medium");
        assert!((assignment.speed - 1.3).abs() < f32::EPSILON);
    }

    #[test]
    fn unknown_speaker_falls_back_to_narrator() {
        let presets = VoicePresets {
            narrator: test_voice_config("en_US-ryan-high", 0.85, 0.95),
            characters: HashMap::new(),
        };
        let router = VoiceRouter::new(Some(&presets), &minimal_audio_config());
        let assignment = router.route("Some Random NPC");

        assert_eq!(assignment.voice_id, "en_US-ryan-high");
    }

    #[test]
    fn kokoro_model_inferred() {
        let presets = VoicePresets {
            narrator: test_voice_config("en_male_deep", 1.0, 0.95),
            characters: HashMap::new(),
        };
        let router = VoiceRouter::new(Some(&presets), &minimal_audio_config());
        let assignment = router.route("narrator");

        assert_eq!(assignment.engine, "kokoro");
        assert_eq!(assignment.voice_id, "en_male_deep");
    }

    #[test]
    fn has_explicit_voice_checks() {
        let presets = VoicePresets {
            narrator: test_voice_config("en_US-lessac-high", 1.0, 1.0),
            characters: HashMap::from([
                ("warrior".to_string(), test_voice_config("en_US-ryan-high", 0.9, 0.9)),
            ]),
        };
        let router = VoiceRouter::new(Some(&presets), &minimal_audio_config());

        assert!(router.has_explicit_voice("narrator"));
        assert!(router.has_explicit_voice("Warrior"));
        assert!(!router.has_explicit_voice("unknown npc"));
    }

    #[test]
    fn voice_count_includes_all_sources() {
        let presets = VoicePresets {
            narrator: test_voice_config("en_US-lessac-high", 1.0, 1.0),
            characters: HashMap::from([
                ("warrior".to_string(), test_voice_config("en_US-ryan-high", 0.9, 0.9)),
                ("mage".to_string(), test_voice_config("en_US-arctic-medium", 1.1, 1.0)),
            ]),
        };
        let mut audio = minimal_audio_config();
        audio.creature_voice_presets.insert(
            "dragon".to_string(),
            test_creature_preset("dragon", 0.6, 0.7),
        );

        let router = VoiceRouter::new(Some(&presets), &audio);
        assert_eq!(router.voice_count(), 4); // narrator + 2 characters + 1 creature
    }
}
