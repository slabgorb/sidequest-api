//! Subject extraction — parse narration for image render subjects.
//!
//! Extracts structured `RenderSubject` data from narrative text using
//! heuristic pattern matching and tier classification. Feeds into the
//! render queue (story 4-3/4-4) to decide what gets rendered.
//!
//! Story 4-2: Subject extraction — parse narration for image render subjects,
//! tier classification.

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
        if !(0.0..=1.0).contains(&narrative_weight) {
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
    tier_rules: TierRules,
}

impl SubjectExtractor {
    /// Create a new extractor with default patterns and rules.
    pub fn new() -> Self {
        Self::with_tier_rules(TierRules::default())
    }

    /// Create an extractor with custom tier rules.
    pub fn with_tier_rules(tier_rules: TierRules) -> Self {
        Self { tier_rules }
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
    pub fn extract(&self, narration: &str, context: &ExtractionContext) -> Option<RenderSubject> {
        let trimmed = narration.trim();
        if trimmed.is_empty() || narration.len() > MAX_NARRATION_LENGTH {
            return None;
        }

        // 1. Entity extraction — match known NPCs, filter recently rendered
        let entities: Vec<String> = context
            .known_npcs
            .iter()
            .filter(|npc| narration.contains(npc.as_str()))
            .filter(|npc| !context.recent_subjects.contains(npc))
            .cloned()
            .collect();

        // 2. Scene type classification
        let scene_type = classify_scene(narration, context);

        // 3. Narrative weight scoring
        let weight = compute_weight(narration, &entities, context);
        if weight < self.tier_rules.minimum_weight {
            return None;
        }

        // 4. Tier assignment
        let tier = assign_tier(&entities, narration);

        // 5. Prompt fragment composition
        let prompt = compose_prompt(narration, &entities, &context.current_location);

        RenderSubject::new(entities, scene_type, tier, prompt, weight)
    }
}

fn classify_scene(narration: &str, context: &ExtractionContext) -> SceneType {
    if context.in_combat {
        return SceneType::Combat;
    }

    let lower = narration.to_lowercase();

    let speech_verbs = [
        "says", "said", "speaks", "spoke", "tells", "told", "growls", "growled",
        "whispers", "whispered", "shouts", "shouted", "asks", "asked",
        "replies", "replied", "demands", "demanded",
    ];
    let has_speech = speech_verbs.iter().any(|v| lower.contains(v));
    let has_quotes = narration.contains('\'') || narration.contains('"');
    if has_speech && has_quotes {
        return SceneType::Dialogue;
    }

    if lower.contains("discover")
        || lower.contains("uncover")
        || lower.contains("runes")
        || lower.contains("hidden")
        || lower.contains("secret")
    {
        return SceneType::Discovery;
    }

    if (lower.contains("leave") && lower.contains("behind")) || lower.contains("step out") {
        return SceneType::Transition;
    }

    SceneType::Exploration
}

fn compute_weight(narration: &str, entities: &[String], context: &ExtractionContext) -> f32 {
    let word_count = narration.split_whitespace().count();
    let length_score = (word_count as f32 / 50.0).min(0.3);

    let lower = narration.to_lowercase();
    let action_words = [
        "swings", "swing", "charges", "charge", "lunges", "lunge", "strikes", "strike",
        "attacks", "attack", "slashes", "slash", "cleaving", "cleave", "dives", "dive",
        "flashing", "flash", "retaliates", "sprays", "spray", "roaring", "roar", "leaps",
        "leap", "surges", "surge", "splits", "screams", "scream", "clash", "whirlwind",
        "flurry", "blood",
    ];
    let action_count = action_words.iter().filter(|w| lower.contains(**w)).count();
    let action_score = (action_count as f32 * 0.05).min(0.3);

    let entity_score = (entities.len() as f32 * 0.1).min(0.2);
    let combat_score = if context.in_combat { 0.15 } else { 0.0 };

    (length_score + action_score + entity_score + combat_score).clamp(0.0, 1.0)
}

fn assign_tier(entities: &[String], narration: &str) -> SubjectTier {
    if entities.len() >= 2 {
        return SubjectTier::Scene;
    }
    if entities.len() == 1 {
        return SubjectTier::Portrait;
    }

    let lower = narration.to_lowercase();

    let landscape_cues = [
        "cavern", "temple", "forest", "clearing", "mountain", "river", "column", "entrance",
        "tower", "cathedral", "stalactite", "valley", "cliff", "ruins", "ocean", "desert",
        "canyon", "cave", "passage", "oaks", "trees",
    ];
    if landscape_cues.iter().any(|c| lower.contains(c)) {
        return SubjectTier::Landscape;
    }

    let abstract_cues = [
        "dread", "tension", "fear", "wonder", "awe", "unease", "foreboding", "chill",
        "unnameable", "nameless", "creeping", "settles over",
    ];
    if abstract_cues.iter().any(|c| lower.contains(c)) {
        return SubjectTier::Abstract;
    }

    SubjectTier::Landscape
}

impl Default for SubjectExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn compose_prompt(narration: &str, entities: &[String], location: &str) -> String {
    let mut parts = Vec::new();

    if !entities.is_empty() {
        parts.push(entities.join(" and "));
    }

    if !location.is_empty() {
        parts.push(format!("at {}", location));
    }

    let excerpt: String = narration.chars().take(120).collect();
    let excerpt = excerpt.trim();
    if !excerpt.is_empty() {
        parts.push(excerpt.to_string());
    }

    let result = parts.join(", ");
    if result.len() < 10 {
        narration.chars().take(100).collect()
    } else {
        result
    }
}
