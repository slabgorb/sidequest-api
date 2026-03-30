//! Scene relevance filter — validate art prompts against current scene context.
//!
//! Before sending an art prompt to the daemon, cross-reference the prompt's
//! subjects against scene state (NPCs present, location, combat status).
//! Reject prompts that reference entities not in the current scene.
//!
//! Story 14-7: Image scene relevance filter.

use tracing::instrument;

use crate::subject::{ExtractionContext, RenderSubject, SceneType};

/// Verdict from scene relevance validation.
///
/// Indicates whether an image prompt should proceed to the daemon
/// or be suppressed due to scene context mismatch.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ImagePromptVerdict {
    /// Prompt matches current scene context — proceed to render.
    Approved,
    /// Prompt references entities or context not in the scene — suppress.
    Rejected {
        /// Human-readable explanation of the mismatch.
        reason: String,
    },
}

impl ImagePromptVerdict {
    /// Returns `true` if the prompt was approved for rendering.
    pub fn is_approved(&self) -> bool {
        matches!(self, ImagePromptVerdict::Approved)
    }

    /// Returns `true` if the prompt was rejected.
    pub fn is_rejected(&self) -> bool {
        matches!(self, ImagePromptVerdict::Rejected { .. })
    }

    /// Returns the rejection reason, or an empty string if approved.
    pub fn reason(&self) -> &str {
        match self {
            ImagePromptVerdict::Approved => "",
            ImagePromptVerdict::Rejected { reason } => reason,
        }
    }

    /// Whether the caller should retry with a new prompt.
    /// Always `false` — rejected prompts are skipped, not regenerated.
    pub fn should_retry(&self) -> bool {
        false
    }
}

/// Stateless validator that checks render subjects against scene context.
///
/// Runs three validation passes:
/// 1. **Entity matching** — subject entities must exist in `known_npcs` (case-insensitive, partial match)
/// 2. **Scene type alignment** — combat prompts rejected when not in combat
/// 3. **Location coherence** — prompt content cross-referenced against current location
#[derive(Debug, Clone)]
pub struct SceneRelevanceValidator;

impl SceneRelevanceValidator {
    /// Create a new stateless validator.
    pub fn new() -> Self {
        Self
    }

    /// Evaluate a render subject against scene context.
    ///
    /// Returns `ImagePromptVerdict::Approved` if the subject matches the scene,
    /// or `ImagePromptVerdict::Rejected` with a reason if it doesn't.
    #[instrument(skip_all, fields(
        entity_count = subject.entities().len(),
        scene_type = ?subject.scene_type(),
        in_combat = context.in_combat,
    ))]
    pub fn evaluate(
        &self,
        subject: &RenderSubject,
        context: &ExtractionContext,
    ) -> ImagePromptVerdict {
        self.evaluate_with_override(subject, context, false)
    }

    /// Evaluate with optional DM override.
    ///
    /// When `dm_override` is `true`, all validation is bypassed and the prompt
    /// is approved unconditionally.
    #[instrument(skip_all, fields(
        entity_count = subject.entities().len(),
        scene_type = ?subject.scene_type(),
        in_combat = context.in_combat,
        dm_override,
    ))]
    pub fn evaluate_with_override(
        &self,
        subject: &RenderSubject,
        context: &ExtractionContext,
        dm_override: bool,
    ) -> ImagePromptVerdict {
        if dm_override {
            tracing::info!("DM override — bypassing scene relevance validation");
            return ImagePromptVerdict::Approved;
        }

        // Pass 1: Scene type alignment — combat prompts need active combat
        if let Some(verdict) = self.check_scene_type(subject, context) {
            tracing::warn!(reason = verdict.reason(), "scene type mismatch");
            return verdict;
        }

        // Pass 2: Entity matching — all subject entities must be in known_npcs
        if let Some(verdict) = self.check_entities(subject, context) {
            tracing::warn!(reason = verdict.reason(), "entity mismatch");
            return verdict;
        }

        // Pass 3: Location coherence — prompt content vs current location
        if let Some(verdict) = self.check_location(subject, context) {
            tracing::warn!(reason = verdict.reason(), "location mismatch");
            return verdict;
        }

        tracing::info!("scene relevance check passed");
        ImagePromptVerdict::Approved
    }

    /// Check that combat scene types only appear during combat.
    fn check_scene_type(
        &self,
        subject: &RenderSubject,
        context: &ExtractionContext,
    ) -> Option<ImagePromptVerdict> {
        if *subject.scene_type() == SceneType::Combat && !context.in_combat {
            return Some(ImagePromptVerdict::Rejected {
                reason: "Combat scene type but no active combat".to_string(),
            });
        }
        None
    }

    /// Check that all subject entities exist in the scene's known NPCs.
    ///
    /// Uses case-insensitive partial matching: "Grok" matches "Grok the Destroyer".
    fn check_entities(
        &self,
        subject: &RenderSubject,
        context: &ExtractionContext,
    ) -> Option<ImagePromptVerdict> {
        let entities = subject.entities();
        if entities.is_empty() {
            return None; // No entities to validate
        }

        let mismatched: Vec<&String> = entities
            .iter()
            .filter(|entity| !entity_matches_any(entity, &context.known_npcs))
            .collect();

        if mismatched.is_empty() {
            None
        } else {
            let names: Vec<&str> = mismatched.iter().map(|s| s.as_str()).collect();
            Some(ImagePromptVerdict::Rejected {
                reason: format!("Entities not in scene: {}", names.join(", ")),
            })
        }
    }

    /// Check that the prompt fragment doesn't reference a location inconsistent
    /// with the current scene location.
    ///
    /// Extracts location-indicative keywords from the prompt and checks if the
    /// current location shares any of them. Only rejects when the prompt strongly
    /// implies a different setting.
    fn check_location(
        &self,
        subject: &RenderSubject,
        context: &ExtractionContext,
    ) -> Option<ImagePromptVerdict> {
        let prompt = subject.prompt_fragment().to_lowercase();
        let location = context.current_location.to_lowercase();

        // Only check if the prompt contains strong location indicators
        // that clearly conflict with the current location
        let location_cues: &[(&str, &[&str])] = &[
            ("forest", &["tavern", "inn", "shop", "market", "city", "town", "arena", "castle", "dungeon"]),
            ("tavern", &["forest", "desert", "ocean", "mountain", "cave", "wilderness", "field", "meadow"]),
            ("desert", &["tavern", "inn", "forest", "ocean", "river", "lake", "swamp"]),
            ("ocean", &["tavern", "inn", "forest", "desert", "mountain", "cave", "dungeon"]),
            ("cave", &["tavern", "inn", "market", "city", "town", "meadow", "field"]),
            ("mountain", &["tavern", "inn", "ocean", "desert", "swamp", "market"]),
            ("dungeon", &["tavern", "inn", "market", "meadow", "field", "forest"]),
        ];

        for (prompt_cue, conflicting_locations) in location_cues {
            if prompt.contains(prompt_cue) {
                for conflict in *conflicting_locations {
                    if location.contains(conflict) {
                        return Some(ImagePromptVerdict::Rejected {
                            reason: format!(
                                "Prompt references '{}' but current location is '{}'",
                                prompt_cue, context.current_location
                            ),
                        });
                    }
                }
            }
        }

        None
    }
}

impl Default for SceneRelevanceValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Case-insensitive partial match: does `entity` match any name in `known_npcs`?
///
/// "Grok" matches "Grok the Destroyer". "grok" matches "Grok".
fn entity_matches_any(entity: &str, known_npcs: &[String]) -> bool {
    let entity_lower = entity.to_lowercase();
    known_npcs.iter().any(|npc| {
        let npc_lower = npc.to_lowercase();
        npc_lower.contains(&entity_lower) || entity_lower.contains(&npc_lower)
    })
}
