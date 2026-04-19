//! Advancement effect resolution and grant mechanics (Story 39-5 / ADR-078).
//!
//! Two public entry points:
//!
//! * [`resolved_beat_for`] — pure view function. Given a character's
//!   `acquired_advancements`, the authored [`BeatDef`], and the genre's
//!   [`AdvancementTree`], compute the effective `edge_delta`,
//!   `target_edge_delta`, and `resource_deltas` for this beat, plus the
//!   list of [`AdvancementEffect`]s that actually applied (for OTEL
//!   attribution).
//!
//! * [`grant_advancement_tier`] — applies a milestone-triggered tier
//!   acquisition. Pushes the tier id into
//!   `CreatureCore.acquired_advancements`, one-shot-applies
//!   `EdgeMaxBonus` effects to `core.edge.max`, and emits the
//!   `advancement.tier_granted` OTEL span. Unknown tier ids fail loudly.
//!
//! Both entry points take immutable references to the
//! [`AdvancementTree`] — the tree is owned by the genre pack.

use std::collections::HashMap;

use sidequest_genre::{AdvancementEffect, AdvancementTier, AdvancementTree, BeatDef};
use sidequest_telemetry::{WatcherEventBuilder, WatcherEventType};

use crate::character::Character;

// ---------------------------------------------------------------------------
// resolved_beat_for
// ---------------------------------------------------------------------------

/// The effective (post-advancement) shape of a beat for a given
/// character.
///
/// `source_effects` names the advancement effects that applied to
/// produce this resolution — the GM panel uses it to explain why a
/// beat cost what it did.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedBeat {
    /// Effective self-debit (may differ from `beat.edge_delta` when a
    /// `BeatDiscount` applies).
    pub edge_delta: Option<i32>,
    /// Effective target-debit (may differ from `beat.target_edge_delta`
    /// when a `LeverageBonus` applies).
    pub target_edge_delta: Option<i32>,
    /// Effective resource deltas (may differ from
    /// `beat.resource_deltas` when a `BeatDiscount.resource_mod`
    /// applies).
    pub resource_deltas: Option<HashMap<String, f64>>,
    /// The effects that actually modified this beat's resolution. Used
    /// by the `advancement.effect_applied` OTEL span and the
    /// `creature.edge_delta.advancements_applied` field.
    pub source_effects: Vec<AdvancementEffect>,
}

/// Resolve a beat's effective shape for a character, given the genre's
/// advancement tree.
///
/// Pure — takes immutable borrows. No state mutation.
///
/// # Panics
///
/// Panics with a clear message if `character.core.acquired_advancements`
/// contains a tier id that the `tree` does not define. This is a
/// save-file or content bug — silently ignoring would mask it. Matches
/// the "no silent fallbacks" rule from `CLAUDE.md`.
pub fn resolved_beat_for(
    character: &Character,
    beat: &BeatDef,
    tree: &AdvancementTree,
) -> ResolvedBeat {
    let mut edge_delta = beat.edge_delta;
    let mut target_edge_delta = beat.target_edge_delta;
    let mut resource_deltas = beat.resource_deltas.clone();
    let mut source_effects: Vec<AdvancementEffect> = Vec::new();

    for tier_id in &character.core.acquired_advancements {
        let tier = tree
            .tiers
            .iter()
            .find(|t| &t.id == tier_id)
            .unwrap_or_else(|| {
                panic!(
                    "unknown advancement tier id '{}' in acquired_advancements — \
                     not present in the genre's advancement tree (tiers: {:?})",
                    tier_id,
                    tree.tiers.iter().map(|t| t.id.as_str()).collect::<Vec<_>>()
                )
            });
        for effect in &tier.effects {
            let applied = apply_effect_to_beat(
                effect,
                beat,
                &mut edge_delta,
                &mut target_edge_delta,
                &mut resource_deltas,
            );
            if applied {
                source_effects.push(effect.clone());
            }
        }
    }

    ResolvedBeat {
        edge_delta,
        target_edge_delta,
        resource_deltas,
        source_effects,
    }
}

/// Apply a single advancement effect to the in-flight resolved beat
/// fields. Returns `true` if the effect actually modified something
/// (so it belongs in `source_effects`).
fn apply_effect_to_beat(
    effect: &AdvancementEffect,
    beat: &BeatDef,
    edge_delta: &mut Option<i32>,
    target_edge_delta: &mut Option<i32>,
    resource_deltas: &mut Option<HashMap<String, f64>>,
) -> bool {
    match effect {
        AdvancementEffect::BeatDiscount {
            beat_id,
            edge_delta_mod,
            resource_mod,
        } if beat_id == &beat.id => {
            let mut applied = false;
            if let Some(cur) = edge_delta.as_mut() {
                *cur += *edge_delta_mod;
                applied = true;
            }
            if let Some(mod_map) = resource_mod {
                if let Some(deltas) = resource_deltas.as_mut() {
                    for (key, delta_mod) in mod_map {
                        if let Some(existing) = deltas.get_mut(key) {
                            // Sign convention: mod is "units of relief"
                            // added to the delta. Author cost -2.0 + mod -1
                            // → resolved -1.0 (less voice spent).
                            *existing -= *delta_mod as f64;
                            applied = true;
                        }
                    }
                }
            }
            applied
        }
        AdvancementEffect::LeverageBonus {
            beat_id,
            target_edge_delta_mod,
        } if beat_id == &beat.id => {
            if let Some(cur) = target_edge_delta.as_mut() {
                *cur += *target_edge_delta_mod;
                true
            } else {
                false
            }
        }
        // EdgeMaxBonus is a grant-time state mutation (see
        // `grant_advancement_tier`), not a per-beat view resolution.
        // EdgeRecovery fires on recovery-trigger dispatch, not beat
        // resolution. LoreRevealBonus is consumed by the lore pipeline
        // at threshold crossings, not by beat dispatch.
        AdvancementEffect::EdgeMaxBonus { .. }
        | AdvancementEffect::EdgeRecovery { .. }
        | AdvancementEffect::LoreRevealBonus { .. }
        | AdvancementEffect::BeatDiscount { .. }
        | AdvancementEffect::LeverageBonus { .. } => false,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// grant_advancement_tier
// ---------------------------------------------------------------------------

/// Result of a successful advancement tier grant.
#[derive(Debug, Clone, PartialEq)]
pub struct AdvancementGrantOutcome {
    /// How much `core.edge.max` changed (sum of `EdgeMaxBonus.amount`
    /// effects on the tier).
    pub edge_max_delta: i32,
    /// The effects that were applied at grant time (currently just
    /// `EdgeMaxBonus` — others resolve elsewhere).
    pub applied_effects: Vec<AdvancementEffect>,
}

/// Errors returned by [`grant_advancement_tier`].
///
/// `#[non_exhaustive]` — future milestone flow may add class-gate or
/// prerequisite failure variants.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AdvancementGrantError {
    /// The tier id was not found in the genre's advancement tree.
    /// No silent fallback — callers must either resolve the mismatch
    /// or escalate.
    #[error("unknown advancement tier id '{tier_id}' — not present in the genre's tree")]
    UnknownTierId {
        /// The missing tier id.
        tier_id: String,
    },
}

/// Grant an advancement tier to a character (Story 39-5 / AC5).
///
/// Pushes the tier id into `character.core.acquired_advancements`,
/// one-shot-applies any `EdgeMaxBonus` effects to `core.edge.max`, and
/// emits the `advancement.tier_granted` OTEL span. Unknown tier ids
/// return `UnknownTierId` without mutating any state.
pub fn grant_advancement_tier(
    character: &mut Character,
    tier_id: &str,
    tree: &AdvancementTree,
) -> Result<AdvancementGrantOutcome, AdvancementGrantError> {
    let tier: &AdvancementTier = tree
        .tiers
        .iter()
        .find(|t| t.id == tier_id)
        .ok_or_else(|| AdvancementGrantError::UnknownTierId {
            tier_id: tier_id.to_string(),
        })?;

    let mut edge_max_delta = 0i32;
    let mut applied_effects: Vec<AdvancementEffect> = Vec::new();
    for effect in &tier.effects {
        if let AdvancementEffect::EdgeMaxBonus { amount } = effect {
            character.core.edge.max += *amount;
            edge_max_delta += *amount;
            applied_effects.push(effect.clone());
        }
    }

    character
        .core
        .acquired_advancements
        .push(tier_id.to_string());

    WatcherEventBuilder::new("advancement", WatcherEventType::StateTransition)
        .field("event", "advancement.tier_granted")
        .field("tier_id", tier_id)
        .field("required_milestone", tier.required_milestone.as_str())
        .field("edge_max_delta", edge_max_delta)
        .field("effects_applied", applied_effects.len() as i64)
        .field("character_name", character.core.name.as_str())
        .send();

    Ok(AdvancementGrantOutcome {
        edge_max_delta,
        applied_effects,
    })
}
