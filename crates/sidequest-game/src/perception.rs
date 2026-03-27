//! Perception rewriter — per-character narration variants based on status effects.
//!
//! Story 8-6: Characters with active perceptual effects (blinded, charmed, etc.)
//! receive narration filtered through their perception state. The rewriter takes
//! base narration and produces per-character variants.
//!
//! **Stub module** — types compile but methods are unimplemented (RED phase).

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
    _strategy: Box<dyn RewriteStrategy>,
}

impl PerceptionRewriter {
    /// Create a new rewriter with the given strategy.
    pub fn new(_strategy: Box<dyn RewriteStrategy>) -> Self {
        todo!("Story 8-6: PerceptionRewriter::new")
    }

    /// Rewrite narration for a single affected character.
    pub fn rewrite(
        &self,
        _base_narration: &str,
        _filter: &PerceptionFilter,
        _genre_voice: &str,
    ) -> Result<String, RewriterError> {
        todo!("Story 8-6: PerceptionRewriter::rewrite")
    }

    /// Produce a human-readable description of active effects for prompt composition.
    pub fn describe_effects(_effects: &[PerceptualEffect]) -> String {
        todo!("Story 8-6: PerceptionRewriter::describe_effects")
    }

    /// Rewrite narration for all affected players. Returns a map of
    /// player_id → rewritten narration. Players whose rewrite fails
    /// receive the base narration (graceful degradation per ADR-006).
    pub fn broadcast(
        &self,
        _base_narration: &str,
        _filters: &HashMap<String, PerceptionFilter>,
        _genre_voice: &str,
    ) -> Result<HashMap<String, String>, RewriterError> {
        todo!("Story 8-6: PerceptionRewriter::broadcast")
    }
}
