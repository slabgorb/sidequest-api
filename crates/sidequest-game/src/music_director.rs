//! Music director — mood extraction from narration and AudioCue generation.
//!
//! Reads narrative text and game state to classify the current mood, then
//! selects an appropriate music track from the genre pack and emits an
//! [`AudioCue`] for the client to play.

use std::borrow::Cow;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sidequest_genre::{AudioConfig, MoodTrack, TrackVariation};
pub use sidequest_genre::FactionThemeDef;
use sidequest_telemetry::{Severity, WatcherEventBuilder, WatcherEventType};

use crate::theme_rotator::{RotationConfig, ThemeRotator};

/// Convert a `TrackVariation` to its lowercase string name for OTEL fields.
/// Matches the `#[serde(rename_all = "snake_case")]` serialization used on
/// the AudioVariation YAML side, so the watcher field values round-trip
/// cleanly with the genre pack input format.
fn variation_label(variation: TrackVariation) -> &'static str {
    match variation {
        TrackVariation::Full => "full",
        TrackVariation::Overture => "overture",
        TrackVariation::Ambient => "ambient",
        TrackVariation::Sparse => "sparse",
        TrackVariation::TensionBuild => "tension_build",
        TrackVariation::Resolution => "resolution",
        // `TrackVariation` is #[non_exhaustive]; any future variant that
        // lands without a label update surfaces as "unknown" on the
        // watcher channel rather than silently dropping the event.
        _ => "unknown",
    }
}

/// Emit a `music_director.variation_fallback` watcher event. Called from
/// both silent-degradation branches of `select_variation()`. Keeping this
/// in one place means the event field shape is guaranteed identical
/// between the two branches (so the GM panel filter logic only needs to
/// know one schema). Story 35-13.
fn emit_variation_fallback(
    mood: &str,
    preferred: TrackVariation,
    selected: TrackVariation,
    reason: &'static str,
    full_available: bool,
) {
    WatcherEventBuilder::new("music_director", WatcherEventType::StateTransition)
        .severity(Severity::Warn)
        .field("action", "variation_fallback")
        .field("mood", mood)
        .field("preferred", variation_label(preferred))
        .field("selected", variation_label(selected))
        .field("reason", reason)
        .field("full_available", full_available)
        .send();
}

// ───────────────────────────────────────────────────────────────────
// Core types
// ───────────────────────────────────────────────────────────────────

/// String-keyed mood type. Replaces the old hardcoded `Mood` enum to support
/// genre-specific mood keys (e.g. "standoff", "saloon") alongside the 7 core moods.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct MoodKey(Cow<'static, str>);

impl MoodKey {
    /// Core mood: active combat encounter.
    pub const COMBAT: MoodKey = MoodKey(Cow::Borrowed("combat"));
    /// Core mood: exploring the world.
    pub const EXPLORATION: MoodKey = MoodKey(Cow::Borrowed("exploration"));
    /// Core mood: rising stakes, approaching danger.
    pub const TENSION: MoodKey = MoodKey(Cow::Borrowed("tension"));
    /// Core mood: victory, quest completion.
    pub const TRIUMPH: MoodKey = MoodKey(Cow::Borrowed("triumph"));
    /// Core mood: loss, mourning.
    pub const SORROW: MoodKey = MoodKey(Cow::Borrowed("sorrow"));
    /// Core mood: unknown, investigation.
    pub const MYSTERY: MoodKey = MoodKey(Cow::Borrowed("mystery"));
    /// Core mood: rest, safe haven.
    pub const CALM: MoodKey = MoodKey(Cow::Borrowed("calm"));

    /// The lowercase string key (e.g. "combat", "standoff").
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this is one of the 7 core moods.
    pub fn is_core(&self) -> bool {
        matches!(
            self.as_str(),
            "combat" | "exploration" | "tension" | "triumph" | "sorrow" | "mystery" | "calm"
        )
    }
}

impl From<&str> for MoodKey {
    fn from(s: &str) -> Self {
        MoodKey(Cow::Owned(s.to_lowercase()))
    }
}

impl std::fmt::Debug for MoodKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MoodKey(\"{}\")", self.as_str())
    }
}

impl Serialize for MoodKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MoodKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(MoodKey::from(s.as_str()))
    }
}

/// Backward-compatible type alias. Prefer [`MoodKey`] for new code.
pub type Mood = MoodKey;

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

/// Result of a music evaluation — either a cue to play, or a reason it was suppressed.
#[derive(Debug, Clone)]
pub enum MusicEvalResult {
    /// A cue was produced — play this track.
    Cue(AudioCue),
    /// Mood unchanged and intensity below threshold — intentional suppression.
    Suppressed { mood: String, intensity: f32 },
    /// Track lookup failed — no eligible tracks for this mood/variation combo.
    NoTrackFound { mood: String, variation: String },
}

/// Result of mood classification.
#[derive(Debug, Clone)]
pub struct MoodClassification {
    /// Primary mood detected.
    pub primary: MoodKey,
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
    /// Mood override from active StructuredEncounter (highest priority).
    pub encounter_mood_override: Option<String>,
    /// Whether the player changed location this turn (from StateDelta).
    pub location_changed: bool,
    /// Turns since last location change.
    pub scene_turn_count: u32,
    /// Drama weight from TensionTracker PacingHint (0.0–1.0).
    pub drama_weight: f32,
    /// Whether combat just ended this turn (transition detection).
    pub combat_just_ended: bool,
    /// Whether this is the first turn of the session.
    pub session_start: bool,
}

/// Context for faction-based music selection. Provides location faction,
/// confrontation actor factions, and player reputation for faction theme triggering.
#[derive(Debug, Clone, Default)]
pub struct FactionContext {
    /// Faction controlling the current location (if any).
    pub location_faction: Option<String>,
    /// Factions of actors in an active confrontation.
    pub actor_factions: Vec<String>,
    /// Player reputation with a specific faction: (faction_id, reputation_score).
    pub player_reputation: Option<(String, i32)>,
}

/// OTEL telemetry snapshot for the music director's current state.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MusicTelemetry {
    pub current_mood: Option<String>,
    pub current_track: Option<String>,
    /// Per-mood recently-played track titles (the anti-repetition history).
    pub rotation_history: HashMap<String, Vec<String>>,
    /// All mood keys available in the genre pack.
    pub available_moods: Vec<String>,
    /// Track titles available per mood.
    pub tracks_per_mood: HashMap<String, Vec<String>>,
    /// Current track variation type (e.g. "overture", "ambient", "full").
    pub current_variation: Option<String>,
    /// Reason for variation selection (e.g. "priority_1_overture: session_start").
    pub variation_reason: Option<String>,
}

/// Mood classification with human-readable reasoning for OTEL telemetry.
#[derive(Debug, Clone)]
pub struct MoodClassificationWithReason {
    pub classification: MoodClassification,
    /// Why this mood was chosen (e.g. "state_override: in_combat", "keyword_scoring: tension (score=3.0)").
    pub reason: String,
    /// (mood_key, keyword) pairs that matched in narration text.
    pub keyword_matches: Vec<(String, String)>,
}

// ───────────────────────────────────────────────────────────────────
// MusicDirector
// ───────────────────────────────────────────────────────────────────

/// Evaluates narration and game state to produce mood-based music cues.
pub struct MusicDirector {
    mood_tracks: HashMap<String, Vec<MoodTrack>>,
    /// Per-mood, per-variation themed track index.
    themed_tracks: HashMap<String, HashMap<TrackVariation, Vec<MoodTrack>>>,
    current_mood: Option<MoodKey>,
    current_track: Option<String>,
    current_variation: Option<TrackVariation>,
    variation_reason: Option<String>,
    rotator: ThemeRotator,
    /// Mood alias mappings from genre pack audio.yaml.
    mood_aliases: HashMap<String, String>,
    /// Faction theme definitions from genre pack audio.yaml.
    faction_themes: Vec<FactionThemeDef>,
}

impl MusicDirector {
    /// Create a new MusicDirector from genre pack audio configuration.
    pub fn new(audio_config: &AudioConfig) -> Self {
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

        // Build per-mood, per-variation themed track index
        let mut themed_tracks: HashMap<String, HashMap<TrackVariation, Vec<MoodTrack>>> =
            HashMap::new();
        for theme in &audio_config.themes {
            // Skip theme bundles with no variations. Without this guard,
            // `entry(mood).or_default()` would insert the mood key with an
            // empty inner HashMap, and `select_variation` would later hit a
            // `Some(empty_map)` fall-through that bypasses the else branch
            // and returns `preferred` silently. With the skip, the mood key
            // stays absent from `themed_tracks`, and the missing-mood else
            // branch fires with `reason="mood_not_in_themed_tracks"`.
            // Story 35-13 Pass 3 — CLAUDE.md no-silent-fallbacks.
            if theme.variations.is_empty() {
                tracing::warn!(
                    mood = %theme.mood,
                    theme_name = %theme.name,
                    "audio theme has no variations — skipping themed-track \
                     registration; select_variation will fall through to \
                     mood_tracks with a variation_fallback event"
                );
                continue;
            }
            let mood_map = themed_tracks
                .entry(theme.mood.clone())
                .or_default();
            for variation in &theme.variations {
                let tv = variation.as_variation();
                let energy = match tv {
                    TrackVariation::Ambient => 0.3,
                    TrackVariation::Sparse => 0.2,
                    TrackVariation::TensionBuild => 0.7,
                    TrackVariation::Overture => 0.6,
                    TrackVariation::Resolution => 0.4,
                    TrackVariation::Full => 0.5,
                    _ => 0.5,
                };
                let title = variation
                    .path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&variation.path)
                    .trim_end_matches(".ogg")
                    .trim_end_matches(".mp3")
                    .replace('_', " ");
                mood_map.entry(tv).or_default().push(MoodTrack {
                    path: variation.path.clone(),
                    title,
                    bpm: 100,
                    energy,
                });
            }
        }

        let track_count: usize = mood_tracks.values().map(|v| v.len()).sum();
        tracing::info!(
            moods = mood_tracks.len(),
            tracks = track_count,
            themes = audio_config.themes.len(),
            themed_moods = themed_tracks.len(),
            "MusicDirector initialized with merged mood_tracks + themes"
        );

        Self {
            mood_tracks,
            themed_tracks,
            current_mood: None,
            current_track: None,
            current_variation: None,
            variation_reason: None,
            rotator: ThemeRotator::new(RotationConfig::default()),
            mood_aliases: audio_config.mood_aliases.clone(),
            faction_themes: audio_config.faction_themes.clone(),
        }
    }

    /// Select the appropriate track variation based on mood classification and game context.
    ///
    /// Implements a 6-priority scoring system:
    /// 1. Overture — session start or new location arrival
    /// 2. Resolution — combat just ended or quest completed
    /// 3. TensionBuild — high intensity (>=0.7) or high drama (>=0.7)
    /// 4. Ambient — low intensity (<=0.3) or long scene (turn count >= 4)
    /// 5. Sparse — mid-intensity (0.3-0.5) with low drama (<=0.3)
    /// 6. Full — default fallback
    ///
    /// If the preferred variation has no tracks for the current mood, falls back
    /// to Full, then to any available variation.
    pub fn select_variation(
        &self,
        classification: &MoodClassification,
        ctx: &MoodContext,
    ) -> TrackVariation {
        let mood_key = classification.primary.as_str();

        let preferred = self.score_variation(classification, ctx);

        // Check if the preferred variation has tracks available
        if let Some(mood_variations) = self.themed_tracks.get(mood_key) {
            if mood_variations.contains_key(&preferred) {
                return preferred;
            }
            // Fallback: Full — preferred variation is missing but Full
            // is registered for this mood. CLAUDE.md no-silent-fallbacks:
            // surface on the watcher channel. Story 35-13.
            if preferred != TrackVariation::Full && mood_variations.contains_key(&TrackVariation::Full) {
                tracing::warn!(
                    mood = mood_key,
                    preferred = ?preferred,
                    "variation fallback: preferred not available, using Full"
                );
                emit_variation_fallback(
                    mood_key,
                    preferred,
                    TrackVariation::Full,
                    "preferred_unavailable",
                    true,
                );
                return TrackVariation::Full;
            }
            // Fallback: first available — neither the preferred variation
            // NOR Full is registered. More severe than the previous branch
            // (genre pack theme bundle has only one variation for this
            // mood). Story 35-13.
            if let Some((&first_available, _)) = mood_variations.iter().next() {
                tracing::warn!(
                    mood = mood_key,
                    preferred = ?preferred,
                    selected = ?first_available,
                    "variation fallback: neither preferred nor Full available, using first available"
                );
                emit_variation_fallback(
                    mood_key,
                    preferred,
                    first_available,
                    "only_first_available",
                    false,
                );
                return first_available;
            }
        } else {
            // Outermost fallback: the mood key has no entry in themed_tracks
            // at all. A genre pack registered this mood without a matching
            // theme bundle. The caller's preference is passed through
            // unchanged so evaluate() can fall through to the un-themed
            // mood_tracks pool. CLAUDE.md no-silent-fallbacks: surface the
            // gap on the watcher channel even though there's no themed
            // track to select. Story 35-13 rework.
            tracing::warn!(
                mood = mood_key,
                preferred = ?preferred,
                "variation fallback: mood has no themed_tracks entry at all"
            );
            emit_variation_fallback(
                mood_key,
                preferred,
                preferred,
                "mood_not_in_themed_tracks",
                false,
            );
        }

        // No themed tracks for this mood — return preferred anyway (evaluate
        // will fall through to the un-themed mood_tracks pool).
        preferred
    }

    /// Pure scoring function — returns the ideal variation based on priority table.
    fn score_variation(
        &self,
        classification: &MoodClassification,
        ctx: &MoodContext,
    ) -> TrackVariation {
        // Priority 1: Overture — session start or new location arrival (turn 0)
        if ctx.session_start || (ctx.location_changed && ctx.scene_turn_count == 0) {
            return TrackVariation::Overture;
        }

        // Priority 2: Resolution — combat just ended or quest completed
        if ctx.combat_just_ended || ctx.quest_completed {
            return TrackVariation::Resolution;
        }

        // Priority 3: TensionBuild — high intensity (non-combat) or high drama
        if classification.intensity >= 0.7 || ctx.drama_weight >= 0.7 {
            return TrackVariation::TensionBuild;
        }

        // Priority 4: Ambient — low intensity or lingering in scene
        if classification.intensity <= 0.3 || ctx.scene_turn_count >= 4 {
            return TrackVariation::Ambient;
        }

        // Priority 5: Sparse — mid-intensity with low drama
        if classification.intensity > 0.3
            && classification.intensity <= 0.5
            && ctx.drama_weight <= 0.3
        {
            return TrackVariation::Sparse;
        }

        // Priority 6: Full — default fallback
        TrackVariation::Full
    }

    /// Evaluate narration text and game context, returning an AudioCue if the mood changed.
    pub fn evaluate(&mut self, narration: &str, ctx: &MoodContext) -> MusicEvalResult {
        let span = tracing::info_span!(
            "music_evaluate",
            mood = tracing::field::Empty,
            track_id = tracing::field::Empty,
            action = tracing::field::Empty,
            mood_changed = tracing::field::Empty,
            variation = tracing::field::Empty,
            variation_reason = tracing::field::Empty,
        );
        let _guard = span.enter();

        let classification = self.classify_mood_inner(narration, ctx);
        span.record("mood", classification.primary.as_str());

        // Only emit a cue if mood actually changed (or intensity is very high)
        if self.current_mood.as_ref() == Some(&classification.primary)
            && classification.intensity <= 0.8
        {
            span.record("mood_changed", false);
            return MusicEvalResult::Suppressed {
                mood: classification.primary.as_str().to_string(),
                intensity: classification.intensity,
            };
        }

        span.record("mood_changed", true);

        // Select variation based on narrative context
        let variation = self.select_variation(&classification, ctx);
        let reason = self.describe_variation_reason(&variation, ctx, &classification);
        span.record("variation", tracing::field::debug(&variation));
        span.record("variation_reason", tracing::field::display(&reason));

        // Try themed tracks for the selected variation first
        let track_path = match self
            .select_themed_track(&classification, &variation)
            .or_else(|| self.select_track(&classification).map(|t| t.path.clone()))
        {
            Some(path) => path,
            None => {
                return MusicEvalResult::NoTrackFound {
                    mood: classification.primary.as_str().to_string(),
                    variation: format!("{:?}", variation),
                };
            }
        };

        let action = Self::transition_action(self.current_mood.as_ref(), &classification.primary);
        let volume = Self::intensity_to_volume(classification.intensity);

        span.record("track_id", tracing::field::display(&track_path));
        span.record("action", tracing::field::display(&action));

        let cue = AudioCue {
            channel: AudioChannel::Music,
            action,
            track_id: Some(track_path.clone()),
            volume,
        };

        self.current_mood = Some(classification.primary);
        self.current_track = Some(track_path);
        self.current_variation = Some(variation);
        self.variation_reason = Some(reason);
        MusicEvalResult::Cue(cue)
    }

    /// Evaluate with a pre-computed mood classification. Used when the caller
    /// already knows the mood (e.g. from a confrontation's declared mood string).
    pub fn evaluate_narration_with_classification(
        &mut self,
        classification: &MoodClassification,
        ctx: &MoodContext,
    ) -> MusicEvalResult {
        let mood_key = classification.primary.as_str();

        // Select variation based on narrative context
        let variation = self.select_variation(classification, ctx);

        // Try themed tracks for the selected variation first
        let track_path = match self
            .select_themed_track(classification, &variation)
            .or_else(|| self.select_track(classification).map(|t| t.path.clone()))
        {
            Some(path) => path,
            None => {
                return MusicEvalResult::NoTrackFound {
                    mood: mood_key.to_string(),
                    variation: format!("{:?}", variation),
                };
            }
        };

        let action = Self::transition_action(self.current_mood.as_ref(), &classification.primary);
        let volume = Self::intensity_to_volume(classification.intensity);

        let cue = AudioCue {
            channel: AudioChannel::Music,
            action,
            track_id: Some(track_path.clone()),
            volume,
        };

        self.current_mood = Some(classification.primary.clone());
        self.current_track = Some(track_path);
        self.current_variation = Some(variation);
        MusicEvalResult::Cue(cue)
    }

    /// Evaluate narration with faction context. If a faction theme matches the
    /// faction context (location faction, actor factions, or reputation threshold),
    /// it overrides mood-based selection. Otherwise falls back to normal mood evaluation.
    pub fn evaluate_with_faction(
        &mut self,
        narration: &str,
        mood_ctx: &MoodContext,
        faction_ctx: &FactionContext,
    ) -> MusicEvalResult {
        // Try to find a matching faction theme
        if let Some(theme) = self.find_matching_faction_theme(faction_ctx) {
            let track_path = theme.track.path.clone();
            let faction_id = theme.faction_id.clone();
            let _action = Self::transition_action(self.current_mood.as_ref(), &MoodKey::COMBAT);
            let cue = AudioCue {
                channel: AudioChannel::Music,
                action: AudioAction::FadeIn,
                track_id: Some(track_path.clone()),
                volume: 0.8,
            };
            tracing::info!(
                faction = %faction_id,
                track = %track_path,
                "faction theme selected, overriding mood-based selection"
            );
            self.current_track = Some(track_path);
            return MusicEvalResult::Cue(cue);
        }

        // No faction match — fall back to normal mood-based evaluation
        self.evaluate(narration, mood_ctx)
    }

    /// Find the first faction theme matching the given faction context.
    ///
    /// Priority: location faction → actor factions → reputation threshold.
    fn find_matching_faction_theme(&self, ctx: &FactionContext) -> Option<&FactionThemeDef> {
        // Check location faction
        if let Some(ref loc_faction) = ctx.location_faction {
            if let Some(theme) = self.faction_themes.iter().find(|t| {
                t.faction_id == *loc_faction && t.triggers.location
            }) {
                return Some(theme);
            }
        }

        // Check actor factions (first match wins)
        for actor_faction in &ctx.actor_factions {
            if let Some(theme) = self.faction_themes.iter().find(|t| {
                t.faction_id == *actor_faction && t.triggers.npc_present
            }) {
                return Some(theme);
            }
        }

        // Check reputation threshold
        if let Some((ref faction_id, reputation)) = ctx.player_reputation {
            if let Some(theme) = self.faction_themes.iter().find(|t| {
                t.faction_id == *faction_id
                    && t.triggers
                        .reputation_threshold
                        .map_or(false, |thresh| reputation >= thresh)
            }) {
                return Some(theme);
            }
        }

        None
    }

    /// Classify the mood from narration text and game state.
    pub fn classify_mood(&self, narration: &str, ctx: &MoodContext) -> MoodClassification {
        let span = tracing::info_span!(
            "music_classify_mood",
            mood = tracing::field::Empty,
            intensity = tracing::field::Empty,
            confidence = tracing::field::Empty,
        );
        let _guard = span.enter();

        let result = self.classify_mood_inner(narration, ctx);
        span.record("mood", result.primary.as_str());
        span.record("intensity", result.intensity as f64);
        span.record("confidence", result.confidence as f64);
        result
    }

    /// Inner classification logic (extracted so span wraps the full result).
    fn classify_mood_inner(&self, _narration: &str, ctx: &MoodContext) -> MoodClassification {
        // Encounter mood override takes highest priority — resolve through alias chain
        if let Some(ref mood_str) = ctx.encounter_mood_override {
            return MoodClassification {
                primary: self.resolve_mood(mood_str),
                intensity: 0.85,
                confidence: 0.95,
            };
        }
        // State-based overrides take priority
        if ctx.in_combat {
            return MoodClassification {
                primary: MoodKey::COMBAT,
                intensity: 0.8,
                confidence: 1.0,
            };
        }
        if ctx.in_chase {
            return MoodClassification {
                primary: MoodKey::TENSION,
                intensity: 0.9,
                confidence: 1.0,
            };
        }
        if ctx.quest_completed {
            return MoodClassification {
                primary: MoodKey::TRIUMPH,
                intensity: 0.7,
                confidence: 0.9,
            };
        }
        if ctx.npc_died {
            return MoodClassification {
                primary: MoodKey::SORROW,
                intensity: 0.7,
                confidence: 0.8,
            };
        }
        // Low health adds tension
        if ctx.party_health_pct > 0.0 && ctx.party_health_pct < 0.3 {
            return MoodClassification {
                primary: MoodKey::TENSION,
                intensity: 0.6,
                confidence: 0.7,
            };
        }

        // No state-based override matched. The dispatch pipeline uses the narrator's
        // scene_mood (structured JSON) for track selection. This classification is only
        // used for OTEL telemetry comparison. Default to Exploration at low confidence
        // so the telemetry clearly shows "no mechanical mood detected."
        MoodClassification {
            primary: MoodKey::EXPLORATION,
            intensity: 0.4,
            confidence: 0.2,
        }
    }

    /// Select a themed track for the given mood and variation.
    /// Uses "{mood}:{variation}" keying for per-variation anti-repetition.
    /// Returns the track path, or None if no themed tracks are available.
    fn select_themed_track(
        &mut self,
        classification: &MoodClassification,
        variation: &TrackVariation,
    ) -> Option<String> {
        let mood_key = classification.primary.as_str();
        let tracks = self
            .themed_tracks
            .get(mood_key)?
            .get(variation)?;
        if tracks.is_empty() {
            return None;
        }
        // Use "{mood}:{variation}" keying for per-variation anti-repetition
        let rotator_key = format!("{mood_key}:{variation:?}").to_lowercase();
        self.rotator
            .select(&rotator_key, tracks, classification.intensity)
            .map(|t| t.path.clone())
    }

    /// Generate a human-readable reason for the variation selection (for OTEL telemetry).
    fn describe_variation_reason(
        &self,
        variation: &TrackVariation,
        ctx: &MoodContext,
        classification: &MoodClassification,
    ) -> String {
        match variation {
            TrackVariation::Overture if ctx.session_start => {
                "priority_1_overture: session_start".to_string()
            }
            TrackVariation::Overture => {
                "priority_1_overture: location_arrival".to_string()
            }
            TrackVariation::Resolution if ctx.combat_just_ended => {
                "priority_2_resolution: combat_just_ended".to_string()
            }
            TrackVariation::Resolution => {
                "priority_2_resolution: quest_completed".to_string()
            }
            TrackVariation::TensionBuild if classification.intensity >= 0.7 => {
                format!("priority_3_tension_build: intensity={:.1}", classification.intensity)
            }
            TrackVariation::TensionBuild => {
                format!("priority_3_tension_build: drama_weight={:.1}", ctx.drama_weight)
            }
            TrackVariation::Ambient if classification.intensity <= 0.3 => {
                format!("priority_4_ambient: low_intensity={:.1}", classification.intensity)
            }
            TrackVariation::Ambient => {
                format!("priority_4_ambient: scene_turn_count={}", ctx.scene_turn_count)
            }
            TrackVariation::Sparse => {
                format!(
                    "priority_5_sparse: intensity={:.1} drama={:.1}",
                    classification.intensity, ctx.drama_weight
                )
            }
            TrackVariation::Full => "priority_6_full: default_fallback".to_string(),
            _ => format!("unknown_variation: {variation:?}"),
        }
    }

    /// Select a track for the classified mood using the theme rotator (legacy flat lookup).
    /// Tries the primary key first, then resolves through alias chain, then hardcoded
    /// fallbacks for genre packs that use different names.
    fn select_track(&mut self, classification: &MoodClassification) -> Option<&MoodTrack> {
        let mood_key = classification.primary.as_str();

        // Try primary key first
        if let Some(tracks) = self.mood_tracks.get(mood_key) {
            return self.rotator.select(mood_key, tracks, classification.intensity);
        }

        // Try resolving through alias chain to find tracks
        let resolved = self.resolve_mood(mood_key);
        let resolved_key = resolved.as_str();
        if resolved_key != mood_key {
            if let Some(tracks) = self.mood_tracks.get(resolved_key) {
                return self.rotator.select(resolved_key, tracks, classification.intensity);
            }
        }

        // Legacy hardcoded fallbacks for genre packs that use different names
        let fallbacks: &[&str] = match mood_key {
            "calm" => &["rest", "teahouse"],
            "mystery" => &["spirit", "tension"],
            "exploration" => &["teahouse"],
            _ => &[],
        };
        if let Some((alias, tracks)) = fallbacks.iter().find_map(|alias| {
            self.mood_tracks.get(*alias).map(|t| (*alias, t))
        }) {
            return self.rotator.select(alias, tracks, classification.intensity);
        }

        None
    }

    /// Determine the audio transition action based on mood change.
    fn transition_action(old: Option<&MoodKey>, new: &MoodKey) -> AudioAction {
        match old {
            None => AudioAction::FadeIn,
            Some(old_mood) if old_mood == &MoodKey::COMBAT && *new != MoodKey::COMBAT => {
                AudioAction::FadeOut
            }
            _ if *new == MoodKey::COMBAT => AudioAction::Play,
            _ => AudioAction::FadeIn,
        }
    }

    /// Map mood intensity (0.0–1.0) to volume (0.3–1.0).
    fn intensity_to_volume(intensity: f32) -> f32 {
        (0.3 + intensity * 0.7).clamp(0.3, 1.0)
    }

    /// Check whether any faction themes are configured.
    pub fn faction_themes_empty(&self) -> bool {
        self.faction_themes.is_empty()
    }

    /// Resolve a mood string through the alias chain to a final MoodKey.
    ///
    /// Resolution order:
    /// 1. If the key matches a core mood, return it directly.
    /// 2. If the key has direct tracks in the genre pack, use the key as-is.
    /// 3. Walk the alias chain (with cycle protection and depth limit of 16).
    /// 4. Fall back to exploration if nothing resolves.
    pub fn resolve_mood(&self, key: &str) -> MoodKey {
        let normalized = key.to_lowercase();

        // Core moods resolve immediately
        if MoodKey::from(normalized.as_str()).is_core() {
            return MoodKey::from(normalized.as_str());
        }

        // If tracks exist for this key directly, use it as-is
        if self.mood_tracks.contains_key(&normalized) {
            return MoodKey::from(normalized.as_str());
        }

        // Walk alias chain with cycle protection
        let mut current = normalized.clone();
        let mut visited = std::collections::HashSet::new();
        visited.insert(current.clone());
        let max_depth = 16;

        for _ in 0..max_depth {
            if let Some(target) = self.mood_aliases.get(&current) {
                let target_lower = target.to_lowercase();
                if visited.contains(&target_lower) {
                    // Cycle detected — fall back to exploration
                    return MoodKey::EXPLORATION;
                }
                visited.insert(target_lower.clone());

                // Check if the alias target is a core mood
                if MoodKey::from(target_lower.as_str()).is_core() {
                    return MoodKey::from(target_lower.as_str());
                }

                // Check if the alias target has direct tracks
                if self.mood_tracks.contains_key(&target_lower) {
                    return MoodKey::from(target_lower.as_str());
                }

                current = target_lower;
            } else {
                // No alias found — fall back to exploration
                return MoodKey::EXPLORATION;
            }
        }

        // Depth limit reached — fall back to exploration
        MoodKey::EXPLORATION
    }

    /// Return the current mood, current track, and per-mood rotation history
    /// for OTEL dashboard telemetry.
    pub fn telemetry_snapshot(&self) -> MusicTelemetry {
        MusicTelemetry {
            current_mood: self.current_mood.as_ref().map(|m| m.as_str().to_string()),
            current_track: self.current_track.clone(),
            rotation_history: self.rotator.history_snapshot(),
            available_moods: self.mood_tracks.keys().cloned().collect(),
            tracks_per_mood: self.mood_tracks.iter()
                .map(|(k, v)| (k.clone(), v.iter().map(|t| t.title.clone()).collect()))
                .collect(),
            current_variation: self.current_variation.map(|v| format!("{v:?}").to_lowercase()),
            variation_reason: self.variation_reason.clone(),
        }
    }

    /// Classify mood and return both the classification result and the keyword matches
    /// that led to it (for OTEL telemetry).
    pub fn classify_mood_with_reasoning(&self, _narration: &str, ctx: &MoodContext) -> MoodClassificationWithReason {
        // Encounter mood override takes highest priority — resolve through alias chain
        if let Some(ref mood_str) = ctx.encounter_mood_override {
            return MoodClassificationWithReason {
                classification: MoodClassification {
                    primary: self.resolve_mood(mood_str),
                    intensity: 0.85,
                    confidence: 0.95,
                },
                reason: format!("encounter_override: {}", mood_str),
                keyword_matches: vec![],
            };
        }
        // State-based overrides
        if ctx.in_combat {
            return MoodClassificationWithReason {
                classification: MoodClassification { primary: MoodKey::COMBAT, intensity: 0.8, confidence: 1.0 },
                reason: "state_override: in_combat".to_string(),
                keyword_matches: vec![],
            };
        }
        if ctx.in_chase {
            return MoodClassificationWithReason {
                classification: MoodClassification { primary: MoodKey::TENSION, intensity: 0.9, confidence: 1.0 },
                reason: "state_override: in_chase".to_string(),
                keyword_matches: vec![],
            };
        }
        if ctx.quest_completed {
            return MoodClassificationWithReason {
                classification: MoodClassification { primary: MoodKey::TRIUMPH, intensity: 0.7, confidence: 0.9 },
                reason: "state_override: quest_completed".to_string(),
                keyword_matches: vec![],
            };
        }
        if ctx.npc_died {
            return MoodClassificationWithReason {
                classification: MoodClassification { primary: MoodKey::SORROW, intensity: 0.7, confidence: 0.8 },
                reason: "state_override: npc_died".to_string(),
                keyword_matches: vec![],
            };
        }
        if ctx.party_health_pct > 0.0 && ctx.party_health_pct < 0.3 {
            return MoodClassificationWithReason {
                classification: MoodClassification { primary: MoodKey::TENSION, intensity: 0.6, confidence: 0.7 },
                reason: format!("state_override: low_health ({}%)", (ctx.party_health_pct * 100.0) as u8),
                keyword_matches: vec![],
            };
        }

        // No state-based override. Narrator's scene_mood is used for track selection
        // in the dispatch pipeline. This telemetry classification defaults to Exploration.
        MoodClassificationWithReason {
            classification: MoodClassification { primary: MoodKey::EXPLORATION, intensity: 0.4, confidence: 0.2 },
            reason: "default: no state override, defer to narrator scene_mood".to_string(),
            keyword_matches: vec![],
        }
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
            mood_keywords: HashMap::new(),
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
            mixer_defaults: None,
            mood_aliases: HashMap::new(),
            faction_themes: Vec::new(),
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
        assert_eq!(classification.primary, MoodKey::COMBAT);
        assert_eq!(classification.confidence, 1.0);

        // Should also produce a cue
        let result = director.evaluate("A gentle breeze", &ctx);
        let cue = match result {
            MusicEvalResult::Cue(c) => c,
            other => panic!("Expected Cue, got {:?}", other),
        };
        assert_eq!(cue.channel, AudioChannel::Music);
        assert!(cue.track_id.unwrap().contains("combat"));
    }

    #[test]
    fn no_state_override_defaults_to_exploration() {
        let config = test_audio_config();
        let director = MusicDirector::new(&config);

        // Without state overrides (in_combat, in_chase, etc.), mood defaults to
        // Exploration regardless of narration content. The dispatch pipeline uses
        // the narrator's scene_mood for track selection, not keyword classification.
        let ctx = MoodContext::default();
        let classification = director.classify_mood(
            "The warrior draws his sword and charges into the fight, clashing blades",
            &ctx,
        );
        assert_eq!(classification.primary, MoodKey::EXPLORATION);
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
        let result1 = director.evaluate("Combat begins!", &ctx);
        assert!(matches!(result1, MusicEvalResult::Cue(_)));

        // Same mood, low intensity — suppressed
        let result2 = director.evaluate("The battle continues.", &ctx);
        assert!(
            matches!(result2, MusicEvalResult::Suppressed { .. }),
            "Same mood should be suppressed unless intensity >= 0.8, got {:?}", result2
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
        let cue = match director.evaluate("Fight!", &ctx) {
            MusicEvalResult::Cue(c) => c,
            other => panic!("Expected Cue, got {:?}", other),
        };
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
        let cue = match director.evaluate("Ambush!", &combat_ctx) {
            MusicEvalResult::Cue(c) => c,
            other => panic!("Expected Cue, got {:?}", other),
        };
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
        let cue = match director.evaluate("The enemies are defeated. You walk on.", &explore_ctx) {
            MusicEvalResult::Cue(c) => c,
            other => panic!("Expected Cue on mood change, got {:?}", other),
        };
        assert_eq!(
            cue.action,
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
        assert_eq!(classification.primary, MoodKey::EXPLORATION);
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
        assert_eq!(classification.primary, MoodKey::TENSION);
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
        assert_eq!(classification.primary, MoodKey::TRIUMPH);
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
