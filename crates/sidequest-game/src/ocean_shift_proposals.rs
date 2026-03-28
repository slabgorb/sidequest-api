//! OCEAN shift proposals — maps game events to personality shifts.
//!
//! Story 10-6: stub module. The mapping logic is not yet implemented.

use sidequest_genre::OceanDimension;

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
///
/// # Stub
/// Currently returns an empty vec — GREEN phase will implement the mapping.
pub fn propose_ocean_shifts(_event: PersonalityEvent, _npc_name: &str) -> Vec<OceanShiftProposal> {
    Vec::new()
}
