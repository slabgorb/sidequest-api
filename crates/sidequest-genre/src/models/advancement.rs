//! Authored advancement effects (Story 39-5 / ADR-078).
//!
//! This module hosts the ADR-078 advancement system types. An
//! [`AdvancementTree`] is a genre's catalogue of authored tiers; each
//! [`AdvancementTier`] is unlocked by an ADR-021 milestone and carries a
//! list of [`AdvancementEffect`]s that modify runtime mechanics (Edge
//! capacity, Edge recovery, beat costs, target-Edge leverage, lore
//! reveal scope).
//!
//! [`RecoveryTrigger`] also lives here because `AdvancementEffect::EdgeRecovery`
//! references it; hosting the canonical definition in the genre crate keeps
//! `sidequest-game` (which owns `EdgePool`) out of a cyclic dependency with
//! `sidequest-genre`. The game crate re-exports it for backward compat with
//! code that referenced `sidequest_game::creature_core::RecoveryTrigger`
//! before the move.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// RecoveryTrigger (moved from sidequest-game::creature_core in 39-5)
// ---------------------------------------------------------------------------

/// Trigger that grants composure back to an `EdgePool`.
///
/// Authored in genre YAML (39-6 pact push currencies, 39-5 advancement
/// effects) and resolved during beat dispatch (39-4).
///
/// `#[non_exhaustive]` so adding further recovery variants in later
/// stories does not break downstream `match` arms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RecoveryTrigger {
    /// Restore edge when the encounter resolves (win or escape).
    OnResolution,
    /// An ally spending an action to shore up the creature.
    OnAllyRescue,
    /// A specific authored beat landing, optionally gated on the
    /// creature being strained (`current <= max / 4`).
    OnBeatSuccess {
        /// Beat identifier that triggers the recovery.
        beat_id: String,
        /// How much edge to restore.
        amount: i32,
        /// If true, only fires while the pool is in the strained band
        /// (`current <= max / 4`). Matches the "Cracked" UI state.
        #[serde(default)]
        while_strained: bool,
    },
}

// ---------------------------------------------------------------------------
// LoreRevealScope — scope for the `LoreRevealBonus` effect (GM amendment)
// ---------------------------------------------------------------------------

/// Scope of a Lore-revealing advancement effect.
///
/// `#[non_exhaustive]` — ADR-079 may extend this set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LoreRevealScope {
    /// Lore minted when EdgeThresholds cross.
    ThresholdCrossings,
    /// Lore minted on encounter resolution.
    EncounterResolution,
    /// Lore minted at session summary.
    SessionSummary,
}

// ---------------------------------------------------------------------------
// AdvancementEffect — the five v1 variants ratified in ADR-078
// ---------------------------------------------------------------------------

/// A mechanical effect that an acquired advancement tier grants.
///
/// Five v1 variants were ratified in ADR-078 (GM amendments,
/// 2026-04-15). Four additional variants (`AllyBeatDiscount`,
/// `BetweenConfrontationsAction`, `AllyEdgeGrant`, `EdgeThresholdDelay`)
/// were deferred to ADR-079.
///
/// `#[non_exhaustive]` ensures external `match` arms stay honest about
/// the open set — new variants can be added without a breaking change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdvancementEffect {
    /// Raise `core.edge.max` by `amount` on grant (one-shot state
    /// mutation — see [`grant_advancement_tier`] in `sidequest-game`).
    EdgeMaxBonus {
        /// Amount to raise `core.edge.max`.
        amount: i32,
    },
    /// Add a new [`RecoveryTrigger`] to the creature's pool (or amplify
    /// an existing one by `amount`, depending on trigger shape).
    EdgeRecovery {
        /// The trigger to add.
        trigger: RecoveryTrigger,
        /// Recovery amount (interpreted by dispatch when the trigger
        /// fires).
        amount: i32,
    },
    /// Reduce the `edge_delta` (and optionally `resource_deltas`) of a
    /// specific beat for the acting character.
    BeatDiscount {
        /// The beat this discount applies to.
        beat_id: String,
        /// Modifier added to `beat.edge_delta` (negative = cheaper).
        edge_delta_mod: i32,
        /// Per-resource modifier added to `beat.resource_deltas` (delta
        /// sign convention — `voice: -1` on a beat with
        /// `resource_deltas.voice = -2.0` yields resolved `-1.0`).
        /// GM amendment (2026-04-15): Pact-affinity tiers use this to
        /// make push currencies cheaper.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resource_mod: Option<HashMap<String, i32>>,
    },
    /// Increase the `target_edge_delta` of a specific beat (more
    /// pressure on the opponent).
    LeverageBonus {
        /// The beat this bonus applies to.
        beat_id: String,
        /// Modifier added to `beat.target_edge_delta` (positive = more
        /// composure pressure).
        target_edge_delta_mod: i32,
    },
    /// Broaden the scope of Lore reveals attributable to this character.
    /// GM amendment (2026-04-15) — accepted in v1.
    LoreRevealBonus {
        /// Which reveal events this bonus widens.
        scope: LoreRevealScope,
    },
}

// ---------------------------------------------------------------------------
// AdvancementTree / AdvancementTier
// ---------------------------------------------------------------------------

/// A genre's authored advancement tiers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvancementTree {
    /// Tiers authored in genre YAML (either on affinity tiers or in a
    /// sibling `advancements.yaml` file). See
    /// [`crate::load_advancement_tree`] for the loader rules.
    pub tiers: Vec<AdvancementTier>,
}

/// A single authored advancement tier.
///
/// Deserialised via `try_from = "RawAdvancementTier"` so invariants
/// (`id` non-blank) are enforced on both the `::new` and `Deserialize`
/// paths — the lang-review rule #13 (constructor/Deserialize
/// consistency) applies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawAdvancementTier", deny_unknown_fields)]
pub struct AdvancementTier {
    /// Tier identifier (stored in `CreatureCore.acquired_advancements`).
    /// Must be non-blank; blank ids would collide with the "no tier"
    /// state and are rejected by [`AdvancementTier::try_from`].
    pub id: String,
    /// The ADR-021 milestone track that grants this tier.
    pub required_milestone: String,
    /// Class restrictions (empty = universal).
    #[serde(default)]
    pub class_gates: Vec<String>,
    /// Effects this tier grants on acquisition.
    #[serde(default)]
    pub effects: Vec<AdvancementEffect>,
}

/// Raw deserialization shape used to validate invariants before
/// constructing [`AdvancementTier`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAdvancementTier {
    id: String,
    required_milestone: String,
    #[serde(default)]
    class_gates: Vec<String>,
    #[serde(default)]
    effects: Vec<AdvancementEffect>,
}

impl TryFrom<RawAdvancementTier> for AdvancementTier {
    type Error = AdvancementTierError;

    fn try_from(raw: RawAdvancementTier) -> Result<Self, Self::Error> {
        if raw.id.trim().is_empty() {
            return Err(AdvancementTierError::BlankId);
        }
        if raw.required_milestone.trim().is_empty() {
            return Err(AdvancementTierError::BlankRequiredMilestone);
        }
        Ok(Self {
            id: raw.id,
            required_milestone: raw.required_milestone,
            class_gates: raw.class_gates,
            effects: raw.effects,
        })
    }
}

/// Errors surfaced by [`AdvancementTier::try_from`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AdvancementTierError {
    /// Tier id is blank (would collide with "no tier acquired").
    #[error("AdvancementTier.id must not be blank")]
    BlankId,
    /// required_milestone is blank.
    #[error("AdvancementTier.required_milestone must not be blank")]
    BlankRequiredMilestone,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_trigger_on_beat_success_defaults_while_strained_false() {
        let yaml = "kind: on_beat_success\nbeat_id: strike\namount: 1\n";
        let t: RecoveryTrigger = serde_yaml::from_str(yaml).expect("deserialize");
        assert!(
            matches!(
                t,
                RecoveryTrigger::OnBeatSuccess {
                    beat_id: ref b,
                    amount: 1,
                    while_strained: false,
                } if b == "strike"
            ),
            "while_strained must default to false when omitted; got {:?}",
            t
        );
    }

    #[test]
    fn advancement_tier_rejects_blank_id() {
        let yaml = "id: \"\"\nrequired_milestone: iron_track\nclass_gates: []\neffects: []\n";
        let result: Result<AdvancementTier, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }
}
