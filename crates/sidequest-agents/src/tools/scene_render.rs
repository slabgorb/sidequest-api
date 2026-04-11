//! Scene render validation tool (ADR-057 Phase 5).
//!
//! Validates subject, tier, mood, and tags parameters and returns a `VisualScene`.
//! This replaces the narrator's `visual_scene` JSON field with a typed tool call.

use crate::orchestrator::VisualScene;

/// Render tier — determines image dimensions and composition.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneTier {
    /// Tight portrait of a single subject.
    Portrait,
    /// Wide environmental landscape shot.
    Landscape,
    /// Mid-distance scene with multiple subjects and setting.
    SceneIllustration,
}

impl SceneTier {
    /// Return the lower-snake-case wire string for this tier.
    pub fn as_str(&self) -> &'static str {
        match self {
            SceneTier::Portrait => "portrait",
            SceneTier::Landscape => "landscape",
            SceneTier::SceneIllustration => "scene_illustration",
        }
    }

    fn from_str_ci(input: &str) -> Option<Self> {
        match input.to_lowercase().as_str() {
            "portrait" => Some(SceneTier::Portrait),
            "landscape" => Some(SceneTier::Landscape),
            "scene_illustration" => Some(SceneTier::SceneIllustration),
            _ => None,
        }
    }
}

/// Visual mood — the emotional atmosphere for image generation.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VisualMood {
    /// Foreboding, threatening atmosphere.
    Ominous,
    /// Suspenseful, on-edge atmosphere.
    Tense,
    /// Magical, otherworldly atmosphere.
    Mystical,
    /// High-stakes, theatrical atmosphere.
    Dramatic,
    /// Sorrowful, somber atmosphere.
    Melancholic,
    /// General environmental ambience without strong emotional cue.
    Atmospheric,
}

impl VisualMood {
    /// Return the lower-snake-case wire string for this mood.
    pub fn as_str(&self) -> &'static str {
        match self {
            VisualMood::Ominous => "ominous",
            VisualMood::Tense => "tense",
            VisualMood::Mystical => "mystical",
            VisualMood::Dramatic => "dramatic",
            VisualMood::Melancholic => "melancholic",
            VisualMood::Atmospheric => "atmospheric",
        }
    }

    fn from_str_ci(input: &str) -> Option<Self> {
        match input.to_lowercase().as_str() {
            "ominous" => Some(VisualMood::Ominous),
            "tense" => Some(VisualMood::Tense),
            "mystical" => Some(VisualMood::Mystical),
            "dramatic" => Some(VisualMood::Dramatic),
            "melancholic" => Some(VisualMood::Melancholic),
            "atmospheric" => Some(VisualMood::Atmospheric),
            _ => None,
        }
    }
}

/// Visual tag — content/style hints for the image renderer.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VisualTag {
    /// Combat or violence content.
    Combat,
    /// Magical/supernatural content.
    Magic,
    /// Special-effect emphasis (explosions, glows, etc.).
    SpecialEffect,
    /// Character-focused composition.
    Character,
    /// Location/environment-focused composition.
    Location,
    /// Atmospheric/environmental ambience.
    Atmosphere,
}

impl VisualTag {
    /// Return the lower-snake-case wire string for this tag.
    pub fn as_str(&self) -> &'static str {
        match self {
            VisualTag::Combat => "combat",
            VisualTag::Magic => "magic",
            VisualTag::SpecialEffect => "special_effect",
            VisualTag::Character => "character",
            VisualTag::Location => "location",
            VisualTag::Atmosphere => "atmosphere",
        }
    }

    fn from_str_ci(input: &str) -> Option<Self> {
        match input.to_lowercase().as_str() {
            "combat" => Some(VisualTag::Combat),
            "magic" => Some(VisualTag::Magic),
            "special_effect" => Some(VisualTag::SpecialEffect),
            "character" => Some(VisualTag::Character),
            "location" => Some(VisualTag::Location),
            "atmosphere" => Some(VisualTag::Atmosphere),
            _ => None,
        }
    }
}

/// Error returned when scene_render validation fails.
#[derive(Debug, thiserror::Error)]
pub enum SceneRenderError {
    /// Subject string was empty, too long, or otherwise invalid.
    #[error("invalid subject: {0}")]
    InvalidSubject(String),
    /// Tier did not match a known `SceneTier` variant.
    #[error("invalid tier: \"{0}\" — expected one of: portrait, landscape, scene_illustration")]
    InvalidTier(String),
    /// Mood did not match a known `VisualMood` variant.
    #[error("invalid mood: \"{0}\" — expected one of: ominous, tense, mystical, dramatic, melancholic, atmospheric")]
    InvalidMood(String),
    /// Tag did not match a known `VisualTag` variant.
    #[error("invalid tag: \"{0}\" — expected one of: combat, magic, special_effect, character, location, atmosphere")]
    InvalidTag(String),
}

/// Validate scene render parameters and return a `VisualScene`.
///
/// - `subject`: free text from the narrator (1–100 chars, passed through as-is)
/// - `tier`: must match a `SceneTier` variant (case-insensitive)
/// - `mood`: must match a `VisualMood` variant (case-insensitive)
/// - `tags`: each must match a `VisualTag` variant (case-insensitive)
#[tracing::instrument(name = "tool.scene_render", skip_all, fields(subject = %subject, tier = %tier))]
pub fn validate_scene_render(
    subject: &str,
    tier: &str,
    mood: &str,
    tags: &[&str],
) -> Result<VisualScene, SceneRenderError> {
    // Validate subject length
    if subject.is_empty() {
        tracing::warn!(
            valid = false,
            "scene_render validation failed: empty subject"
        );
        return Err(SceneRenderError::InvalidSubject(
            "subject must not be empty".to_string(),
        ));
    }
    if subject.len() > 100 {
        tracing::warn!(
            valid = false,
            length = subject.len(),
            "scene_render validation failed: subject too long"
        );
        return Err(SceneRenderError::InvalidSubject(format!(
            "subject must be ≤100 chars, got {}",
            subject.len()
        )));
    }

    // Validate tier
    let validated_tier = match SceneTier::from_str_ci(tier) {
        Some(t) => t,
        None => {
            tracing::warn!(
                valid = false,
                tier = tier,
                "scene_render validation failed: invalid tier"
            );
            return Err(SceneRenderError::InvalidTier(tier.to_string()));
        }
    };

    // Validate mood
    let validated_mood = match VisualMood::from_str_ci(mood) {
        Some(m) => m,
        None => {
            tracing::warn!(
                valid = false,
                mood = mood,
                "scene_render validation failed: invalid mood"
            );
            return Err(SceneRenderError::InvalidMood(mood.to_string()));
        }
    };

    // Validate tags
    let mut validated_tags = Vec::with_capacity(tags.len());
    for tag in tags {
        match VisualTag::from_str_ci(tag) {
            Some(vt) => validated_tags.push(vt.as_str().to_string()),
            None => {
                tracing::warn!(
                    valid = false,
                    tag = *tag,
                    "scene_render validation failed: invalid tag"
                );
                return Err(SceneRenderError::InvalidTag((*tag).to_string()));
            }
        }
    }

    let scene = VisualScene {
        subject: subject.to_string(),
        tier: validated_tier.as_str().to_string(),
        mood: validated_mood.as_str().to_string(),
        tags: validated_tags,
    };

    tracing::info!(
        valid = true,
        tier = validated_tier.as_str(),
        mood = validated_mood.as_str(),
        tag_count = tags.len(),
        "scene_render validated"
    );

    Ok(scene)
}
