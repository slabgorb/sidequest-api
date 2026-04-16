//! Archetype resolution engine — walks the base→genre→world inheritance chain
//! to resolve [jungian, rpg_role] axis pairs into named archetypes.

use crate::error::GenreError;
use crate::models::archetype_axes::BaseArchetypes;
use crate::models::archetype_constraints::{ArchetypeConstraints, PairingWeight};
use crate::models::archetype_funnels::ArchetypeFunnels;

/// The resolved output of the archetype pipeline.
#[derive(Debug, Clone)]
pub struct ResolvedArchetype {
    /// The display name (from funnel, genre fallback, or axis IDs).
    pub name: String,
    /// Jungian axis value.
    pub jungian: String,
    /// RPG role axis value.
    pub rpg_role: String,
    /// NPC role axis value (None for PCs).
    pub npc_role: Option<String>,
    /// Faction from funnel, if any.
    pub faction: Option<String>,
    /// Lore description from funnel or genre flavor.
    pub lore: String,
    /// Cultural status from funnel.
    pub cultural_status: Option<String>,
    /// Pairing weight classification.
    pub weight: PairingWeight,
    /// Source of the name resolution.
    pub resolution_source: ResolutionSource,
}

/// Where the resolved name came from.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolutionSource {
    /// Name came from a world-level funnel.
    WorldFunnel,
    /// Name came from genre-level fallback.
    GenreFallback,
}

/// Resolve a [jungian, rpg_role] pair through the full inheritance chain.
///
/// Resolution order:
/// 1. Check base layer — both axis values must exist
/// 2. Check constraints — pairing must not be forbidden
/// 3. Check world funnels — if a funnel claims it, use that name
/// 4. Fall back to genre fallback name
pub fn resolve_archetype(
    jungian: &str,
    rpg_role: &str,
    base: &BaseArchetypes,
    constraints: &ArchetypeConstraints,
    funnels: Option<&ArchetypeFunnels>,
) -> Result<ResolvedArchetype, GenreError> {
    // Step 1: Validate axis values exist in base
    if !base.jungian.iter().any(|j| j.id == jungian) {
        return Err(GenreError::ValidationError {
            message: format!("Unknown Jungian archetype: '{jungian}'"),
        });
    }
    if !base.rpg_roles.iter().any(|r| r.id == rpg_role) {
        return Err(GenreError::ValidationError {
            message: format!("Unknown RPG role: '{rpg_role}'"),
        });
    }

    // Step 2: Check genre-level constraints
    let weight = constraints
        .pairing_weight(jungian, rpg_role)
        .unwrap_or(PairingWeight::Uncommon);

    if weight == PairingWeight::Forbidden {
        return Err(GenreError::ValidationError {
            message: format!("Forbidden pairing: [{jungian}, {rpg_role}]"),
        });
    }

    // Step 2b: Check world-level forbidden
    if let Some(funnels) = funnels {
        if funnels.is_forbidden(jungian, rpg_role) {
            return Err(GenreError::ValidationError {
                message: format!("World-forbidden pairing: [{jungian}, {rpg_role}]"),
            });
        }
    }

    // Step 3: Try world funnel
    if let Some(funnels) = funnels {
        if let Some(funnel) = funnels.resolve(jungian, rpg_role) {
            return Ok(ResolvedArchetype {
                name: funnel.name.clone(),
                jungian: jungian.to_string(),
                rpg_role: rpg_role.to_string(),
                npc_role: None,
                faction: funnel.faction.clone(),
                lore: funnel.lore.clone(),
                cultural_status: funnel.cultural_status.clone(),
                weight,
                resolution_source: ResolutionSource::WorldFunnel,
            });
        }
    }

    // Step 4: Genre fallback
    let fallback_name = constraints
        .fallback_name(rpg_role)
        .unwrap_or(rpg_role)
        .to_string();

    Ok(ResolvedArchetype {
        name: fallback_name,
        jungian: jungian.to_string(),
        rpg_role: rpg_role.to_string(),
        npc_role: None,
        faction: None,
        lore: String::new(),
        cultural_status: None,
        weight,
        resolution_source: ResolutionSource::GenreFallback,
    })
}
