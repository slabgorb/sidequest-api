//! OCEAN shift proposals — maps game events to personality shifts.
//!
//! Story 10-6: event-to-shift mapping logic.
//! Story 15-2: wiring — event detection from narration + application to game state.

use sidequest_genre::{OceanDimension, OceanShiftLog};

use crate::npc::NpcRegistryEntry;

/// Narrative events that can trigger personality evolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonalityEvent {
    /// A trusted NPC or ally betrayed the character.
    Betrayal,
    /// The character nearly died.
    NearDeath,
    /// The character achieved a significant victory.
    Victory,
    /// The character suffered a defeat or significant loss.
    Defeat,
    /// The character formed a meaningful social bond.
    SocialBonding,
}

/// A proposed OCEAN personality shift for a specific NPC.
#[derive(Debug, Clone)]
pub struct OceanShiftProposal {
    /// Name of the NPC whose personality would shift.
    pub npc_name: String,
    /// Which OCEAN dimension to shift.
    pub dimension: OceanDimension,
    /// Magnitude and direction of the shift (capped at abs <= 2.0).
    pub delta: f64,
    /// Narrative reason for the shift.
    pub cause: String,
}

/// Given a personality-relevant event and an NPC name, return proposed OCEAN
/// shifts.
pub fn propose_ocean_shifts(event: PersonalityEvent, npc_name: &str) -> Vec<OceanShiftProposal> {
    let name = npc_name.to_string();
    match event {
        PersonalityEvent::Betrayal => vec![
            OceanShiftProposal {
                npc_name: name.clone(),
                dimension: OceanDimension::Agreeableness,
                delta: -1.5,
                cause: "betrayed by a trusted ally".to_string(),
            },
            OceanShiftProposal {
                npc_name: name,
                dimension: OceanDimension::Neuroticism,
                delta: 1.0,
                cause: "emotional fallout from betrayal".to_string(),
            },
        ],
        PersonalityEvent::NearDeath => vec![
            OceanShiftProposal {
                npc_name: name,
                dimension: OceanDimension::Neuroticism,
                delta: 1.5,
                cause: "near-death experience".to_string(),
            },
        ],
        PersonalityEvent::Victory => vec![
            OceanShiftProposal {
                npc_name: name.clone(),
                dimension: OceanDimension::Conscientiousness,
                delta: 1.0,
                cause: "bolstered confidence from victory".to_string(),
            },
            OceanShiftProposal {
                npc_name: name,
                dimension: OceanDimension::Extraversion,
                delta: 0.5,
                cause: "emboldened by triumph".to_string(),
            },
        ],
        PersonalityEvent::Defeat => vec![
            OceanShiftProposal {
                npc_name: name.clone(),
                dimension: OceanDimension::Neuroticism,
                delta: 1.0,
                cause: "shaken by defeat".to_string(),
            },
            OceanShiftProposal {
                npc_name: name,
                dimension: OceanDimension::Extraversion,
                delta: -1.0,
                cause: "withdrawn after loss".to_string(),
            },
        ],
        PersonalityEvent::SocialBonding => vec![
            OceanShiftProposal {
                npc_name: name.clone(),
                dimension: OceanDimension::Agreeableness,
                delta: 1.0,
                cause: "formed a meaningful social bond".to_string(),
            },
            OceanShiftProposal {
                npc_name: name,
                dimension: OceanDimension::Extraversion,
                delta: 0.5,
                cause: "energized by social connection".to_string(),
            },
        ],
    }
}

/// Keyword patterns for each personality event type.
/// Multi-word phrases are preferred to avoid substring false positives.
const EVENT_KEYWORDS: &[(PersonalityEvent, &[&str])] = &[
    (PersonalityEvent::Betrayal, &[
        "betrayal", "betrays", "betrayed", "betray ", "treachery", "backstab",
        "turns on", "turned on", "double-cross",
    ]),
    (PersonalityEvent::NearDeath, &[
        "nearly dies", "nearly died", "near death", "near-death", "barely alive",
        "clinging to life", "brink of death", "mortally wounded", "fatal wound",
        "almost killed", "barely survives", "barely survived",
    ]),
    (PersonalityEvent::Victory, &[
        "victory", "victorious", "triumphant", "triumph", "vanquish", "vanquishing",
        "conquers", "conquered", "prevails", "prevailed", "wins the battle",
        "claims victory", "final blow",
    ]),
    (PersonalityEvent::Defeat, &[
        "crushing defeat", "utterly defeated", "falls in battle", "defeated",
        "vanquished", "overwhelmed", "routed", "suffered a loss", "suffered a defeat",
    ]),
    (PersonalityEvent::SocialBonding, &[
        "bond of friendship", "deep bond", "forged a bond", "forming a deep",
        "friendship", "deep connection", "trust builds",
        "grows closer", "warm embrace", "companionship",
    ]),
];

/// Scan narration text for personality-relevant events involving known NPCs.
///
/// Returns `(npc_name, event)` pairs. Only NPCs in `npc_names` are considered.
/// Uses keyword matching against the full narration text.
pub fn detect_personality_events(
    narration: &str,
    npc_names: &[&str],
) -> Vec<(String, PersonalityEvent)> {
    let narration_lower = narration.to_lowercase();
    let mut results = Vec::new();

    for npc_name in npc_names {
        if !narration_lower.contains(&npc_name.to_lowercase()) {
            continue;
        }

        for &(event, keywords) in EVENT_KEYWORDS {
            if keywords.iter().any(|kw| narration_lower.contains(kw)) {
                results.push((npc_name.to_string(), event));
                break; // one event per NPC per narration
            }
        }
    }

    results
}

/// Apply OCEAN shift proposals to NPCs in the registry.
///
/// For each `(npc_name, event)` pair:
/// 1. Look up the NPC in `registry`
/// 2. Skip if NPC not found or has no OCEAN profile
/// 3. Generate proposals via `propose_ocean_shifts()`
/// 4. Apply each proposal's delta to the NPC's OCEAN profile
/// 5. Regenerate `ocean_summary` from the mutated profile
///
/// Returns applied proposals and the shift log for telemetry.
pub fn apply_ocean_shifts(
    registry: &mut [NpcRegistryEntry],
    events: &[(String, PersonalityEvent)],
    turn: u32,
) -> (Vec<OceanShiftProposal>, OceanShiftLog) {
    let mut applied = Vec::new();
    let mut log = OceanShiftLog::default();

    for (npc_name, event) in events {
        let Some(entry) = registry.iter_mut().find(|e| e.name == *npc_name) else {
            tracing::warn!(npc_name = %npc_name, "ocean_shift: NPC not found in registry, skipping");
            continue;
        };
        let Some(ref mut profile) = entry.ocean else {
            continue;
        };

        let proposals = propose_ocean_shifts(*event, npc_name);
        for proposal in &proposals {
            profile.apply_shift(proposal.dimension, proposal.delta, proposal.cause.clone(), turn, &mut log);
        }
        // Regenerate summary from mutated profile
        entry.ocean_summary = profile.behavioral_summary();
        applied.extend(proposals);
    }

    (applied, log)
}
