//! Perception rewriter — per-character narration variants based on status effects.
//!
//! Story 8-6: Characters with active perceptual effects (blinded, charmed, etc.)
//! receive narration filtered through their perception state. The rewriter takes
//! base narration and produces per-character variants.
//!
//! Wired into dispatch via `session_sync.rs` (ClaudeRewriteStrategy from resonator)
//! and `dispatch/mod.rs` (PerceptionFilter construction from game state).

use std::collections::HashMap;

/// Perceptual status effects that alter how a character perceives narration.
///
/// These are distinct from combat `StatusEffectKind` (Poison, Stun, etc.) —
/// perceptual effects change *what the player reads*, not mechanical outcomes.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PerceptualEffect {
    /// Cannot see — narration filtered through sound, touch, smell.
    Blinded,
    /// Perceives the source as a trusted ally.
    Charmed {
        /// Who charmed this character.
        source: String,
    },
    /// Under another entity's control — narration reflects the controller's will.
    Dominated {
        /// Who controls this character.
        controller: String,
    },
    /// Perceives things that aren't there, misses things that are.
    Hallucinating,
    /// Cannot hear — narration filtered through sight and touch only.
    Deafened,
    /// A genre-specific perceptual effect not covered by the standard variants.
    Custom {
        /// Short name for the effect.
        name: String,
        /// How this effect alters perception.
        description: String,
    },
}

impl PerceptualEffect {
    /// Parse a structured status code into a PerceptualEffect.
    ///
    /// Expected formats (exact, case-insensitive):
    /// - `"blinded"` → Blinded
    /// - `"deafened"` → Deafened
    /// - `"hallucinating"` → Hallucinating
    /// - `"charmed_by:<source>"` → Charmed { source }
    /// - `"dominated_by:<controller>"` → Dominated { controller }
    ///
    /// Returns None for unrecognized codes. Callers MUST emit an OTEL warning
    /// for None — no silent drop. This replaces the keyword `contains()` matching
    /// in dispatch/barrier.rs that violated the Zork Problem (ADR-010/032).
    pub fn from_status_code(code: &str) -> Option<Self> {
        let lower = code.to_lowercase();
        let lower = lower.trim();
        match lower {
            "blinded" => Some(Self::Blinded),
            "deafened" => Some(Self::Deafened),
            "hallucinating" => Some(Self::Hallucinating),
            _ if lower.starts_with("charmed_by:") => {
                let source = lower
                    .strip_prefix("charmed_by:")
                    .unwrap_or("unknown")
                    .trim()
                    .to_string();
                Some(Self::Charmed { source })
            }
            _ if lower.starts_with("dominated_by:") => {
                let controller = lower
                    .strip_prefix("dominated_by:")
                    .unwrap_or("unknown")
                    .trim()
                    .to_string();
                Some(Self::Dominated { controller })
            }
            _ => None,
        }
    }
}

/// Specifies which character is affected and by what effects.
///
/// Fields are private with getters (lang-review rule #9) to preserve
/// future validation invariants.
pub struct PerceptionFilter {
    character_name: String,
    effects: Vec<PerceptualEffect>,
}

impl PerceptionFilter {
    /// Create a new perception filter for a character.
    pub fn new(character_name: String, effects: Vec<PerceptualEffect>) -> Self {
        Self {
            character_name,
            effects,
        }
    }

    /// The name of the affected character.
    pub fn character_name(&self) -> &str {
        &self.character_name
    }

    /// The active perceptual effects on this character.
    pub fn effects(&self) -> &[PerceptualEffect] {
        &self.effects
    }

    /// Whether this character has any active perceptual effects.
    pub fn has_effects(&self) -> bool {
        !self.effects.is_empty()
    }
}

/// Trait for the rewrite strategy — allows swapping Claude for a test double.
pub trait RewriteStrategy {
    /// Rewrite narration for a character affected by perceptual effects.
    fn rewrite(
        &self,
        base_narration: &str,
        filter: &PerceptionFilter,
        genre_voice: &str,
    ) -> Result<String, RewriterError>;
}

/// Error type for perception rewriting operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RewriterError {
    /// The underlying agent (Claude) call failed.
    #[error("agent error: {0}")]
    Agent(String),
}

/// Rewrites narration per-character based on active perceptual effects.
///
/// Uses a `RewriteStrategy` trait object so production code can use Claude
/// while tests use a deterministic double.
pub struct PerceptionRewriter {
    strategy: Box<dyn RewriteStrategy>,
}

impl PerceptionRewriter {
    /// Create a new rewriter with the given strategy.
    pub fn new(strategy: Box<dyn RewriteStrategy>) -> Self {
        Self { strategy }
    }

    /// Rewrite narration for a single affected character.
    pub fn rewrite(
        &self,
        base_narration: &str,
        filter: &PerceptionFilter,
        genre_voice: &str,
    ) -> Result<String, RewriterError> {
        self.strategy.rewrite(base_narration, filter, genre_voice)
    }

    /// Produce a human-readable description of active effects for prompt composition.
    pub fn describe_effects(effects: &[PerceptualEffect]) -> String {
        if effects.is_empty() {
            return "none".to_string();
        }
        effects
            .iter()
            .map(|e| match e {
                PerceptualEffect::Blinded => "Blinded (cannot see)".to_string(),
                PerceptualEffect::Charmed { source } => {
                    format!("Charmed by {source} (perceives as trusted ally)")
                }
                PerceptualEffect::Dominated { controller } => {
                    format!("Dominated by {controller} (under their control)")
                }
                PerceptualEffect::Hallucinating => {
                    "Hallucinating (perceives things that aren't there)".to_string()
                }
                PerceptualEffect::Deafened => "Deafened (cannot hear)".to_string(),
                PerceptualEffect::Custom { name, description } => {
                    format!("{name} ({description})")
                }
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Rewrite narration for all affected players. Returns a map of
    /// player_id → rewritten narration. Players whose rewrite fails
    /// receive the base narration (graceful degradation per ADR-006).
    pub fn broadcast(
        &self,
        base_narration: &str,
        filters: &HashMap<String, PerceptionFilter>,
        genre_voice: &str,
    ) -> Result<HashMap<String, String>, RewriterError> {
        let mut results = HashMap::new();
        for (player_id, filter) in filters {
            let narration = match self.strategy.rewrite(base_narration, filter, genre_voice) {
                Ok(rewritten) => rewritten,
                Err(_) => base_narration.to_string(),
            };
            results.insert(player_id.clone(), narration);
        }
        Ok(results)
    }
}
