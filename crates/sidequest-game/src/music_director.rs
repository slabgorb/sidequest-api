//! Music director — mood extraction from narration and AudioCue generation.
//!
//! Reads narrative text and game state to classify the current mood, then
//! selects an appropriate music track from the genre pack and emits an
//! [`AudioCue`] for the client to play.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sidequest_genre::{AudioConfig, MoodTrack};

use crate::theme_rotator::{RotationConfig, ThemeRotator};

// ───────────────────────────────────────────────────────────────────
// Core types
// ───────────────────────────────────────────────────────────────────

/// Music mood categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mood {
    /// Active combat encounter.
    Combat,
    /// Exploring the world.
    Exploration,
    /// Rising stakes, approaching danger.
    Tension,
    /// Victory, quest completion.
    Triumph,
    /// Loss, mourning.
    Sorrow,
    /// Unknown, investigation.
    Mystery,
    /// Rest, safe haven.
    Calm,
}

impl Mood {
    /// Return the lowercase string key used in genre pack YAML (e.g. "combat").
    pub fn as_key(&self) -> &'static str {
        match self {
            Mood::Combat => "combat",
            Mood::Exploration => "exploration",
            Mood::Tension => "tension",
            Mood::Triumph => "triumph",
            Mood::Sorrow => "sorrow",
            Mood::Mystery => "mystery",
            Mood::Calm => "calm",
        }
    }
}

/// Audio channel for cue targeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AudioChannel {
    /// Background music.
    Music,
    /// Sound effects.
    Sfx,
    /// Environmental ambience.
    Ambience,
}

impl std::fmt::Display for AudioChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioChannel::Music => write!(f, "music"),
            AudioChannel::Sfx => write!(f, "sfx"),
            AudioChannel::Ambience => write!(f, "ambience"),
        }
    }
}

/// Audio transition action.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AudioAction {
    /// Start playing immediately.
    Play,
    /// Fade in from silence.
    FadeIn,
    /// Fade out to silence.
    FadeOut,
    /// Duck volume for speech.
    Duck,
    /// Restore volume after speech.
    Restore,
    /// Stop playback.
    Stop,
}

impl std::fmt::Display for AudioAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioAction::Play => write!(f, "play"),
            AudioAction::FadeIn => write!(f, "fade_in"),
            AudioAction::FadeOut => write!(f, "fade_out"),
            AudioAction::Duck => write!(f, "duck"),
            AudioAction::Restore => write!(f, "restore"),
            AudioAction::Stop => write!(f, "stop"),
        }
    }
}

/// A command for the client audio system.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AudioCue {
    /// Target audio channel.
    pub channel: AudioChannel,
    /// Transition action.
    pub action: AudioAction,
    /// Track identifier (file path from genre pack).
    pub track_id: Option<String>,
    /// Target volume (0.0–1.0).
    pub volume: f32,
}

/// Result of mood classification.
#[derive(Debug, Clone)]
pub struct MoodClassification {
    /// Primary mood detected.
    pub primary: Mood,
    /// Intensity level (0.0–1.0).
    pub intensity: f32,
    /// Classification confidence (0.0–1.0).
    pub confidence: f32,
}

/// Game state context for mood classification overrides.
#[derive(Debug, Clone, Default)]
pub struct MoodContext {
    /// Whether the party is in active combat.
    pub in_combat: bool,
    /// Whether a chase sequence is active.
    pub in_chase: bool,
    /// Party health as a fraction (0.0–1.0).
    pub party_health_pct: f32,
    /// Whether a quest was just completed this turn.
    pub quest_completed: bool,
    /// Whether an NPC died this turn.
    pub npc_died: bool,
}

// ───────────────────────────────────────────────────────────────────
// MusicDirector
// ───────────────────────────────────────────────────────────────────

/// Evaluates narration and game state to produce mood-based music cues.
pub struct MusicDirector {
    mood_keywords: HashMap<String, Vec<String>>,
    mood_tracks: HashMap<String, Vec<MoodTrack>>,
    current_mood: Option<Mood>,
    current_track: Option<String>,
    rotator: ThemeRotator,
}

impl MusicDirector {
    /// Create a new MusicDirector from genre pack audio configuration.
    pub fn new(audio_config: &AudioConfig) -> Self {
        let mut mood_keywords = audio_config.mood_keywords.clone();
        // Provide sensible defaults if genre pack omits mood_keywords
        if mood_keywords.is_empty() {
            mood_keywords = Self::default_mood_keywords();
        }

        // Start with mood_tracks from the genre pack
        let mut mood_tracks = audio_config.mood_tracks.clone();

        // Merge themes.variations into mood_tracks — themes contain set-1/set-2
        // variations (ambient, full, overture, sparse, tension_build, resolution)
        // that mood_tracks doesn't include.
        for theme in &audio_config.themes {
            let mood_key = &theme.mood;
            let tracks = mood_tracks.entry(mood_key.clone()).or_default();
            for variation in &theme.variations {
                // Skip if this path is already in mood_tracks (avoid duplicates)
                if tracks.iter().any(|t| t.path == variation.path) {
                    continue;
                }
                // Derive energy from variation type
                let energy = match variation.variation_type.as_str() {
                    "ambient" => 0.3,
                    "sparse" => 0.2,
                    "tension_build" => 0.7,
                    "overture" => 0.6,
                    "resolution" => 0.4,
                    "full" => 0.5,
                    _ => 0.5,
                };
                // Derive title from filename
                let title = variation
                    .path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&variation.path)
                    .trim_end_matches(".ogg")
                    .trim_end_matches(".mp3")
                    .replace('_', " ");
                tracks.push(MoodTrack {
                    path: variation.path.clone(),
                    title,
                    bpm: theme.variations.first().map_or(100, |_| 100), // BPM not in variations
                    energy,
                });
            }
        }

        let track_count: usize = mood_tracks.values().map(|v| v.len()).sum();
        tracing::info!(
            moods = mood_tracks.len(),
            tracks = track_count,
            themes = audio_config.themes.len(),
            "MusicDirector initialized with merged mood_tracks + themes"
        );

        Self {
            mood_keywords,
            mood_tracks,
            current_mood: None,
            current_track: None,
            rotator: ThemeRotator::new(RotationConfig::default()),
        }
    }

    /// Evaluate narration text and game context, returning an AudioCue if the mood changed.
    pub fn evaluate(&mut self, narration: &str, ctx: &MoodContext) -> Option<AudioCue> {
        let classification = self.classify_mood(narration, ctx);

        // Only emit a cue if mood actually changed (or intensity is very high)
        if self.current_mood.as_ref() == Some(&classification.primary)
            && classification.intensity <= 0.8
        {
            return None;
        }

        let track = self.select_track(&classification)?;
        let track_path = track.path.clone();

        let action = Self::transition_action(self.current_mood.as_ref(), &classification.primary);
        let volume = Self::intensity_to_volume(classification.intensity);

        let cue = AudioCue {
            channel: AudioChannel::Music,
            action,
            track_id: Some(track_path.clone()),
            volume,
        };

        self.current_mood = Some(classification.primary);
        self.current_track = Some(track_path);
        Some(cue)
    }

    /// Classify the mood from narration text and game state.
    pub fn classify_mood(&self, narration: &str, ctx: &MoodContext) -> MoodClassification {
        // State-based overrides take priority
        if ctx.in_combat {
            return MoodClassification {
                primary: Mood::Combat,
                intensity: 0.8,
                confidence: 1.0,
            };
        }
        if ctx.in_chase {
            return MoodClassification {
                primary: Mood::Tension,
                intensity: 0.9,
                confidence: 1.0,
            };
        }
        if ctx.quest_completed {
            return MoodClassification {
                primary: Mood::Triumph,
                intensity: 0.7,
                confidence: 0.9,
            };
        }
        if ctx.npc_died {
            return MoodClassification {
                primary: Mood::Sorrow,
                intensity: 0.7,
                confidence: 0.8,
            };
        }
        // Low health adds tension
        if ctx.party_health_pct > 0.0 && ctx.party_health_pct < 0.3 {
            return MoodClassification {
                primary: Mood::Tension,
                intensity: 0.6,
                confidence: 0.7,
            };
        }

        // Keyword scoring
        let lower = narration.to_lowercase();
        let mut scores: HashMap<&str, f32> = HashMap::new();

        for (mood_key, keywords) in &self.mood_keywords {
            let count = keywords.iter().filter(|kw| lower.contains(kw.as_str())).count();
            if count > 0 {
                *scores.entry(mood_key.as_str()).or_default() += count as f32;
            }
        }

        // Highest scoring mood wins
        if let Some((mood_key, score)) = scores
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        {
            let primary = Self::key_to_mood(mood_key);
            return MoodClassification {
                primary,
                intensity: (score / 5.0).clamp(0.3, 1.0),
                confidence: (score / 3.0).clamp(0.0, 1.0),
            };
        }

        // Default: Exploration
        MoodClassification {
            primary: Mood::Exploration,
            intensity: 0.4,
            confidence: 0.2,
        }
    }

    /// Select a track for the classified mood using the theme rotator.
    /// Tries the primary key first, then genre pack aliases (e.g. "rest" for "calm").
    fn select_track(&mut self, classification: &MoodClassification) -> Option<&MoodTrack> {
        let mood_key = classification.primary.as_key();
        // Try primary key, then fallback aliases for genre packs that use different names
        let fallbacks: &[&str] = match classification.primary {
            Mood::Calm => &["rest", "teahouse"],
            Mood::Mystery => &["spirit", "tension"],
            Mood::Exploration => &["teahouse"],
            _ => &[],
        };
        // Track which key the tracks actually came from so the rotator
        // records history under the correct bucket.
        let (actual_key, tracks) = if let Some(t) = self.mood_tracks.get(mood_key) {
            (mood_key, t)
        } else if let Some((alias, t)) = fallbacks.iter().find_map(|alias| {
            self.mood_tracks.get(*alias).map(|t| (*alias, t))
        }) {
            (alias, t)
        } else {
            return None;
        };
        self.rotator
            .select(actual_key, tracks, classification.intensity)
    }

    /// Determine the audio transition action based on mood change.
    fn transition_action(old: Option<&Mood>, new: &Mood) -> AudioAction {
        match (old, new) {
            (None, _) => AudioAction::FadeIn,
            (Some(Mood::Combat), m) if *m != Mood::Combat => AudioAction::FadeOut,
            (_, Mood::Combat) => AudioAction::Play,
            _ => AudioAction::FadeIn,
        }
    }

    /// Map mood intensity (0.0–1.0) to volume (0.3–1.0).
    fn intensity_to_volume(intensity: f32) -> f32 {
        (0.3 + intensity * 0.7).clamp(0.3, 1.0)
    }

    /// Convert a string mood key to the Mood enum.
    /// Handles genre pack aliases (e.g. "rest" → Calm, "spirit" → Mystery).
    fn key_to_mood(key: &str) -> Mood {
        match key {
            "combat" => Mood::Combat,
            "tension" => Mood::Tension,
            "triumph" => Mood::Triumph,
            "sorrow" => Mood::Sorrow,
            "mystery" | "spirit" => Mood::Mystery,
            "calm" | "rest" | "teahouse" => Mood::Calm,
            _ => Mood::Exploration,
        }
    }

    /// Default mood keywords when genre pack doesn't provide them.
    fn default_mood_keywords() -> HashMap<String, Vec<String>> {
        let mut kw = HashMap::new();
        kw.insert(
            "combat".to_string(),
            vec![
                "attack", "sword", "strike", "fight", "battle", "clash", "slash",
                "punch", "blood", "wound", "damage", "charge", "parry", "dodge",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );
        kw.insert(
            "tension".to_string(),
            vec![
                "danger", "lurk", "shadow", "creep", "ominous", "threat", "dread",
                "suspense", "trap", "ambush", "stalk", "menace",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );
        kw.insert(
            "triumph".to_string(),
            vec![
                "victory", "triumph", "celebrate", "succeed", "glory", "champion",
                "conquer", "reward", "treasure",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );
        kw.insert(
            "sorrow".to_string(),
            vec![
                "death", "mourn", "grief", "loss", "weep", "fallen", "farewell",
                "tragic", "bury",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );
        kw.insert(
            "mystery".to_string(),
            vec![
                "strange", "mystery", "puzzle", "riddle", "hidden", "secret",
                "ancient", "rune", "arcane", "whisper",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );
        kw.insert(
            "calm".to_string(),
            vec![
                "rest", "sleep", "peaceful", "tavern", "inn", "campfire", "heal",
                "quiet", "serene", "safe",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );
        kw.insert(
            "exploration".to_string(),
            vec![
                "walk", "travel", "road", "path", "forest", "mountain", "river",
                "village", "arrive", "discover", "enter",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );
        kw
    }
}

impl std::fmt::Debug for MusicDirector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MusicDirector")
            .field("current_mood", &self.current_mood)
            .field("current_track", &self.current_track)
            .field("mood_count", &self.mood_tracks.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sidequest_genre::MixerConfig;

    fn test_audio_config() -> AudioConfig {
        let mut mood_tracks = HashMap::new();
        mood_tracks.insert(
            "combat".to_string(),
            vec![
                MoodTrack {
                    path: "audio/music/combat_1.ogg".to_string(),
                    title: "Battle Drums".to_string(),
                    bpm: 140,
                    energy: 0.9,
                },
                MoodTrack {
                    path: "audio/music/combat_2.ogg".to_string(),
                    title: "War March".to_string(),
                    bpm: 120,
                    energy: 0.7,
                },
            ],
        );
        mood_tracks.insert(
            "exploration".to_string(),
            vec![MoodTrack {
                path: "audio/music/explore_1.ogg".to_string(),
                title: "Wanderer's Path".to_string(),
                bpm: 90,
                energy: 0.4,
            }],
        );
        mood_tracks.insert(
            "tension".to_string(),
            vec![MoodTrack {
                path: "audio/music/tension_1.ogg".to_string(),
                title: "Dark Shadows".to_string(),
                bpm: 100,
                energy: 0.6,
            }],
        );
        mood_tracks.insert(
            "triumph".to_string(),
            vec![MoodTrack {
                path: "audio/music/triumph_1.ogg".to_string(),
                title: "Victory Fanfare".to_string(),
                bpm: 130,
                energy: 0.8,
            }],
        );

        AudioConfig {
            mood_tracks,
            sfx_library: HashMap::new(),
            creature_voice_presets: HashMap::new(),
            mixer: MixerConfig {
                music_volume: 0.6,
                sfx_volume: 0.8,
                voice_volume: 1.0,
                duck_music_for_voice: true,
                duck_amount_db: -12.0,
                crossfade_default_ms: 3000,
            },
            themes: vec![],
            ai_generation: None,
            mood_keywords: HashMap::new(), // Use defaults
            mixer_defaults: None,
        }
    }

    #[test]
    fn combat_context_forces_combat_mood() {
        let config = test_audio_config();
        let mut director = MusicDirector::new(&config);

        let ctx = MoodContext {
            in_combat: true,
            ..Default::default()
        };
        let classification = director.classify_mood("A gentle breeze blows through the meadow", &ctx);
        assert_eq!(classification.primary, Mood::Combat);
        assert_eq!(classification.confidence, 1.0);

        // Should also produce a cue
        let cue = director.evaluate("A gentle breeze", &ctx);
        assert!(cue.is_some());
        let cue = cue.unwrap();
        assert_eq!(cue.channel, AudioChannel::Music);
        assert!(cue.track_id.unwrap().contains("combat"));
    }

    #[test]
    fn keyword_scoring_picks_combat() {
        let config = test_audio_config();
        let director = MusicDirector::new(&config);

        let ctx = MoodContext::default();
        let classification = director.classify_mood(
            "The warrior draws his sword and charges into the fight, clashing blades",
            &ctx,
        );
        assert_eq!(classification.primary, Mood::Combat);
    }

    #[test]
    fn same_mood_no_new_cue() {
        let config = test_audio_config();
        let mut director = MusicDirector::new(&config);

        let ctx = MoodContext {
            in_combat: true,
            ..Default::default()
        };

        // First evaluation produces a cue
        let cue1 = director.evaluate("Combat begins!", &ctx);
        assert!(cue1.is_some());

        // Same mood, low intensity — no new cue
        let cue2 = director.evaluate("The battle continues.", &ctx);
        assert!(
            cue2.is_none(),
            "Same mood should not produce a new cue unless intensity >= 0.8"
        );
    }

    #[test]
    fn track_from_genre_pack() {
        let config = test_audio_config();
        let mut director = MusicDirector::new(&config);

        let ctx = MoodContext {
            in_combat: true,
            ..Default::default()
        };
        let cue = director.evaluate("Fight!", &ctx).unwrap();
        let track = cue.track_id.unwrap();
        assert!(
            track.contains("combat"),
            "Track should come from genre pack combat tracks, got: {}",
            track
        );
    }

    #[test]
    fn combat_start_uses_play() {
        let config = test_audio_config();
        let mut director = MusicDirector::new(&config);

        // First set a non-combat mood
        let explore_ctx = MoodContext::default();
        director.evaluate("Walking down the forest path", &explore_ctx);

        // Now switch to combat
        let combat_ctx = MoodContext {
            in_combat: true,
            ..Default::default()
        };
        let cue = director.evaluate("Ambush!", &combat_ctx).unwrap();
        assert_eq!(cue.action, AudioAction::Play, "Combat start should use Play (immediate)");
    }

    #[test]
    fn combat_end_uses_fadeout() {
        let config = test_audio_config();
        let mut director = MusicDirector::new(&config);

        // Start in combat
        let combat_ctx = MoodContext {
            in_combat: true,
            ..Default::default()
        };
        director.evaluate("Battle!", &combat_ctx);

        // End combat → exploration
        let explore_ctx = MoodContext::default();
        let cue = director.evaluate("The enemies are defeated. You walk on.", &explore_ctx);
        assert!(cue.is_some());
        assert_eq!(
            cue.unwrap().action,
            AudioAction::FadeOut,
            "Combat → non-combat should use FadeOut"
        );
    }

    #[test]
    fn volume_from_intensity() {
        // Low intensity
        let vol_low = MusicDirector::intensity_to_volume(0.0);
        assert!((vol_low - 0.3).abs() < 0.01);

        // High intensity
        let vol_high = MusicDirector::intensity_to_volume(1.0);
        assert!((vol_high - 1.0).abs() < 0.01);

        // Mid intensity
        let vol_mid = MusicDirector::intensity_to_volume(0.5);
        assert!(vol_mid > 0.5 && vol_mid < 0.8);
    }

    #[test]
    fn default_mood_is_exploration() {
        let config = test_audio_config();
        let director = MusicDirector::new(&config);

        let ctx = MoodContext::default();
        let classification = director.classify_mood("Some unclassifiable text about nothing in particular", &ctx);
        assert_eq!(classification.primary, Mood::Exploration);
    }

    #[test]
    fn chase_forces_tension() {
        let config = test_audio_config();
        let director = MusicDirector::new(&config);

        let ctx = MoodContext {
            in_chase: true,
            ..Default::default()
        };
        let classification = director.classify_mood("Running through meadows", &ctx);
        assert_eq!(classification.primary, Mood::Tension);
    }

    #[test]
    fn quest_complete_forces_triumph() {
        let config = test_audio_config();
        let director = MusicDirector::new(&config);

        let ctx = MoodContext {
            quest_completed: true,
            ..Default::default()
        };
        let classification = director.classify_mood("You hand over the letter", &ctx);
        assert_eq!(classification.primary, Mood::Triumph);
    }

    #[test]
    fn audio_cue_serializes() {
        let cue = AudioCue {
            channel: AudioChannel::Music,
            action: AudioAction::FadeIn,
            track_id: Some("audio/combat.ogg".to_string()),
            volume: 0.8,
        };
        let json = serde_json::to_value(&cue).unwrap();
        assert_eq!(json["channel"], "Music");
        assert_eq!(json["action"], "FadeIn");
        let vol = json["volume"].as_f64().unwrap();
        assert!((vol - 0.8).abs() < 0.001, "Volume should be ~0.8, got {}", vol);
    }
}
