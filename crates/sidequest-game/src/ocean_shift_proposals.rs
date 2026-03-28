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
