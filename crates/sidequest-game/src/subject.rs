//! Subject extraction — parse narration for image render subjects.
//!
//! Extracts structured `RenderSubject` data from narrative text using
//! heuristic pattern matching and tier classification. Feeds into the
//! render queue (story 4-3/4-4) to decide what gets rendered.
//!
//! Story 4-2: Subject extraction — parse narration for image render subjects,
//! tier classification.

use std::collections::HashMap;

use regex::Regex;

/// Maximum narration input length accepted by the extractor.
/// Inputs longer than this are rejected to prevent unbounded processing (CWE-674).
pub const MAX_NARRATION_LENGTH: usize = 10_000;

/// Subject tier classification for the render pipeline.
///
/// Determines how the daemon composes the image (close-up vs wide shot vs abstract).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SubjectTier {
    /// Single character close-up (1 entity, dialogue/examine context).
    Portrait,
    /// 2-4 entities interacting (combat, conversation).
    Scene,
    /// Environment focus (entering new area, descriptive passage).
    Landscape,
    /// Mood/atmosphere (tension, dread, wonder).
    Abstract,
}

/// Scene type classification from narrative context.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SceneType {
    /// Active combat encounter.
    Combat,
    /// Characters speaking.
    Dialogue,
    /// Traversing or examining the environment.
    Exploration,
    /// Finding something new (treasure, secret, NPC).
    Discovery,
    /// Moving between areas or scenes.
    Transition,
}

/// A render subject extracted from narration text.
///
/// Produced by `SubjectExtractor::extract()`, consumed by the render queue.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderSubject {
    entities: Vec<String>,
    scene_type: SceneType,
    tier: SubjectTier,
    prompt_fragment: String,
    narrative_weight: f32,
}

impl RenderSubject {
    /// Create a new RenderSubject with validated narrative_weight.
    ///
    /// Returns `None` if `narrative_weight` is outside \[0.0, 1.0\].
    pub fn new(
        entities: Vec<String>,
        scene_type: SceneType,
        tier: SubjectTier,
        prompt_fragment: String,
        narrative_weight: f32,
    ) -> Option<Self> {
        if narrative_weight < 0.0 || narrative_weight > 1.0 {
            return None;
        }
        Some(Self {
            entities,
            scene_type,
            tier,
            prompt_fragment,
            narrative_weight,
        })
    }

    /// Extracted entity names.
    pub fn entities(&self) -> &[String] {
        &self.entities
    }

    /// Classified scene type.
    pub fn scene_type(&self) -> &SceneType {
        &self.scene_type
    }

    /// Render tier classification.
    pub fn tier(&self) -> &SubjectTier {
        &self.tier
    }

    /// Daemon-ready image description.
    pub fn prompt_fragment(&self) -> &str {
        &self.prompt_fragment
    }

    /// Narrative significance weight (0.0–1.0).
    pub fn narrative_weight(&self) -> f32 {
        self.narrative_weight
    }
}

/// Game state context for resolving entities during extraction.
#[derive(Debug, Clone, Default)]
pub struct ExtractionContext {
    /// Known NPC names in the current scene.
    pub known_npcs: Vec<String>,
    /// Current location name.
    pub current_location: String,
    /// Whether combat is active.
    pub in_combat: bool,
    /// Recently rendered subjects (for dedup).
    pub recent_subjects: Vec<String>,
}

/// Tier assignment rules (entity count + scene type → tier).
#[derive(Debug, Clone)]
pub struct TierRules {
    /// Minimum narrative weight to produce a render subject.
    pub minimum_weight: f32,
}

impl Default for TierRules {
    fn default() -> Self {
        Self {
            minimum_weight: 0.2,
        }
    }
}

/// Heuristic subject extractor for narration text.
///
/// Runs a multi-pass pipeline: entity extraction → scene classification →
/// tier assignment → prompt composition → weight scoring.
pub struct SubjectExtractor {
    _entity_patterns: Vec<Regex>,
    _scene_keywords: HashMap<SceneType, Vec<String>>,
    tier_rules: TierRules,
}

impl SubjectExtractor {
    /// Create a new extractor with default patterns and rules.
    pub fn new() -> Self {
        todo!("Story 4-2: implement SubjectExtractor::new")
    }

    /// Create an extractor with custom tier rules.
    pub fn with_tier_rules(tier_rules: TierRules) -> Self {
        todo!("Story 4-2: implement SubjectExtractor::with_tier_rules")
    }

    /// Extract a render subject from narration text.
    ///
    /// Returns `None` if:
    /// - The narration is empty or whitespace-only
    /// - The narration exceeds `MAX_NARRATION_LENGTH`
    /// - The computed narrative weight is below `tier_rules.minimum_weight`
    ///
    /// The extraction pipeline:
    /// 1. Entity extraction (named NPCs from context, pattern matching)
    /// 2. Scene type classification (keyword matching + game state)
    /// 3. Tier assignment (entity count + scene type)
    /// 4. Prompt fragment composition
    /// 5. Narrative weight scoring
    pub fn extract(&self, _narration: &str, _context: &ExtractionContext) -> Option<RenderSubject> {
        todo!("Story 4-2: implement SubjectExtractor::extract")
    }
}
