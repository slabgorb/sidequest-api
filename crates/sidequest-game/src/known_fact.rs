//! KnownFact — play-derived knowledge accumulation.
//!
//! Story 9-3: Characters learn things during play. These facts accumulate
//! in the character's knowledge base and feed into narrator context.
//! Facts are monotonic — no deletion or decay in this epic.

use serde::{Deserialize, Serialize};

/// A fact the character learned during play.
///
/// Unlike backstory, KnownFacts are earned through gameplay. They persist
/// across sessions and feed into narrator context so Claude can reference
/// what the character knows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownFact {
    /// The fact content in genre voice.
    pub content: String,
    /// Turn number when this fact was learned.
    pub learned_turn: u64,
    /// How the fact was acquired.
    pub source: FactSource,
    /// How confident the character is in this fact.
    pub confidence: Confidence,
}

/// How a fact was acquired.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FactSource {
    /// Character saw or sensed something directly.
    Observation,
    /// Told by an NPC or another player.
    Dialogue,
    /// Found via investigation or ability use.
    Discovery,
}

/// How confident the character is in a fact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Confidence {
    /// Confirmed by direct evidence.
    Certain,
    /// Inferred but not confirmed.
    Suspected,
    /// Hearsay — may be wrong.
    Rumored,
}

/// A fact discovered during a turn, tagged with the character who learned it.
///
/// Used in `WorldStatePatch` to carry newly discovered facts from the
/// world state agent back to the game state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredFact {
    /// Name of the character who learned this fact.
    pub character_name: String,
    /// The fact itself.
    pub fact: KnownFact,
}
