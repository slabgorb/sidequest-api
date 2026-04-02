//! Scene render validation tool (ADR-057 Phase 5).
//!
//! Validates subject, tier, mood, and tags parameters and returns a `VisualScene`.
//! This replaces the narrator's `visual_scene` JSON field with a typed tool call.

use crate::orchestrator::VisualScene;

/// Render tier — determines image dimensions and composition.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneTier {
    Portrait,
    Landscape,
    SceneIllustration,
}

impl SceneTier {
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
    Ominous,
    Tense,
    Mystical,
    Dramatic,
    Melancholic,
    Atmospheric,
}

impl VisualMood {
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
    Combat,
    Magic,
    SpecialEffect,
    Character,
    Location,
    Atmosphere,
}

impl VisualTag {
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
    #[error("invalid subject: {0}")]
    InvalidSubject(String),
    #[error("invalid tier: \"{0}\" — expected one of: portrait, landscape, scene_illustration")]
    InvalidTier(String),
    #[error("invalid mood: \"{0}\" — expected one of: ominous, tense, mystical, dramatic, melancholic, atmospheric")]
    InvalidMood(String),
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
        return Err(SceneRenderError::InvalidSubject(
            "subject must not be empty".to_string(),
        ));
    }
    if subject.len() > 100 {
        return Err(SceneRenderError::InvalidSubject(format!(
            "subject must be ≤100 chars, got {}",
            subject.len()
        )));
    }

    // Validate tier
    let validated_tier =
        SceneTier::from_str_ci(tier).ok_or_else(|| SceneRenderError::InvalidTier(tier.to_string()))?;

    // Validate mood
    let validated_mood =
        VisualMood::from_str_ci(mood).ok_or_else(|| SceneRenderError::InvalidMood(mood.to_string()))?;

    // Validate tags
    let mut validated_tags = Vec::with_capacity(tags.len());
    for tag in tags {
        let vt = VisualTag::from_str_ci(tag)
            .ok_or_else(|| SceneRenderError::InvalidTag((*tag).to_string()))?;
        validated_tags.push(vt.as_str().to_string());
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
