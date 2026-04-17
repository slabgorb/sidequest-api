//! Axis-lookup shim for archetype resolution.
//!
//! This is the Phase 1 entry point for resolving `(jungian, rpg_role)`
//! pairs into a named archetype. It takes the same pre-loaded structures
//! the legacy `archetype_resolve::resolve_archetype` took (base archetypes,
//! genre constraints, optional world funnels) and produces an
//! [`ArchetypeResolution`] carrying the new [`ArchetypeResolved`] value
//! plus the lookup metadata (source tier, pairing weight) that callers need.
//!
//! Every successful resolution emits a `content.resolve` OTEL span via the
//! resolver framework so archetype resolution is visible in the GM panel
//! alongside every other axis that migrates in future phases.

use crate::archetype::ArchetypeResolved;
use crate::error::GenreError;
use crate::models::archetype_axes::BaseArchetypes;
use crate::models::archetype_constraints::{ArchetypeConstraints, PairingWeight};
use crate::models::archetype_funnels::ArchetypeFunnels;
use crate::resolver::{emit_content_resolve_span, ContributionKind, MergeStep, Provenance, Tier};
use std::path::PathBuf;
use std::time::Instant;

/// Which tier the archetype's display name came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolutionSource {
    /// Name came from a world-level funnel.
    WorldFunnel,
    /// Name came from genre-level fallback.
    GenreFallback,
}

/// The full result of resolving an archetype pair.
///
/// `resolved` is the Layered framework's archetype value. `source` and
/// `weight` are lookup metadata — not layered fields, because they describe
/// *where* the resolution came from rather than *what* value is at each
/// tier.
#[derive(Debug, Clone)]
pub struct ArchetypeResolution {
    /// The resolved archetype value (name, lore, faction, etc.).
    pub resolved: ArchetypeResolved,
    /// Tier that supplied the final display name.
    pub source: ResolutionSource,
    /// Pairing-weight classification from the genre's constraint table.
    pub weight: PairingWeight,
}

/// Resolve a `(jungian, rpg_role)` pair through the archetype inheritance chain.
///
/// Mirrors the legacy `archetype_resolve::resolve_archetype` behavior:
/// 1. Validate both axis values exist in `base`.
/// 2. Reject forbidden pairings (genre constraints + world funnels).
/// 3. Prefer a world-funnel match; otherwise fall back to the genre's
///    configured fallback name.
///
/// Emits a `content.resolve` OTEL span for observability.
///
/// `genre` / `world` are used only for the span's tier-provenance labels —
/// the actual resolution is driven by the pre-loaded structures. When
/// Phase 2 migrates archetype content onto filesystem fragments, this shim
/// will grow a [`crate::resolver::Resolver`] call to enrich the resolved
/// struct with per-tier YAML data.
pub fn resolve_archetype(
    jungian: &str,
    rpg_role: &str,
    base: &BaseArchetypes,
    constraints: &ArchetypeConstraints,
    funnels: Option<&ArchetypeFunnels>,
    genre: &str,
    world: Option<&str>,
) -> Result<ArchetypeResolution, GenreError> {
    let start = Instant::now();

    // Step 1: validate axis IDs.
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

    // Step 2: genre constraints.
    let weight = constraints
        .pairing_weight(jungian, rpg_role)
        .unwrap_or(PairingWeight::Uncommon);

    if weight == PairingWeight::Forbidden {
        return Err(GenreError::ValidationError {
            message: format!("Forbidden pairing: [{jungian}, {rpg_role}]"),
        });
    }

    // Step 2b: world-level forbidden.
    if let Some(funnels) = funnels {
        if funnels.is_forbidden(jungian, rpg_role) {
            return Err(GenreError::ValidationError {
                message: format!("World-forbidden pairing: [{jungian}, {rpg_role}]"),
            });
        }
    }

    // Step 3: world funnel lookup.
    let (resolved, source, final_tier, final_file) = if let Some(funnels) = funnels {
        if let Some(funnel) = funnels.resolve(jungian, rpg_role) {
            let resolved = ArchetypeResolved {
                name: funnel.name.clone(),
                jungian: jungian.to_string(),
                rpg_role: rpg_role.to_string(),
                npc_role: None,
                speech_pattern: String::new(),
                lore: funnel.lore.clone(),
                faction: funnel.faction.clone(),
                cultural_status: funnel.cultural_status.clone(),
            };
            let file = PathBuf::from(format!(
                "{}/worlds/{}/archetype_funnels.yaml",
                genre,
                world.unwrap_or("<unknown>")
            ));
            (resolved, ResolutionSource::WorldFunnel, Tier::World, file)
        } else {
            genre_fallback(jungian, rpg_role, constraints, genre)
        }
    } else {
        genre_fallback(jungian, rpg_role, constraints, genre)
    };

    // Build provenance for OTEL emission.
    let provenance = Provenance {
        source_tier: final_tier,
        source_file: final_file.clone(),
        source_span: None,
        merge_trail: vec![MergeStep {
            tier: final_tier,
            file: final_file,
            span: None,
            contribution: ContributionKind::Initial,
        }],
    };

    let elapsed_us = start.elapsed().as_micros() as u64;
    let field_path = format!("archetype.{jungian}.{rpg_role}");
    emit_content_resolve_span(
        "archetype",
        &field_path,
        genre,
        world,
        None,
        &provenance,
        elapsed_us,
    );

    Ok(ArchetypeResolution {
        resolved,
        source,
        weight,
    })
}

fn genre_fallback(
    jungian: &str,
    rpg_role: &str,
    constraints: &ArchetypeConstraints,
    genre: &str,
) -> (ArchetypeResolved, ResolutionSource, Tier, PathBuf) {
    let fallback_name = constraints
        .fallback_name(rpg_role)
        .unwrap_or(rpg_role)
        .to_string();
    let resolved = ArchetypeResolved {
        name: fallback_name,
        jungian: jungian.to_string(),
        rpg_role: rpg_role.to_string(),
        npc_role: None,
        speech_pattern: String::new(),
        lore: String::new(),
        faction: None,
        cultural_status: None,
    };
    let file = PathBuf::from(format!("{genre}/archetype_constraints.yaml"));
    (resolved, ResolutionSource::GenreFallback, Tier::Genre, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_base() -> BaseArchetypes {
        serde_yaml::from_str(
            r#"
            jungian:
              - id: sage
                drive: "Seeks truth"
                ocean_tendencies:
                  openness: [7.0, 9.5]
                  conscientiousness: [6.0, 8.0]
                  extraversion: [2.0, 5.0]
                  agreeableness: [4.0, 7.0]
                  neuroticism: [3.0, 6.0]
                stat_affinity: [wisdom, intellect]
              - id: hero
                drive: "Proves worth"
                ocean_tendencies:
                  openness: [5.0, 7.0]
                  conscientiousness: [6.0, 8.5]
                  extraversion: [6.0, 8.5]
                  agreeableness: [5.0, 7.5]
                  neuroticism: [2.0, 4.5]
                stat_affinity: [strength, endurance]
            rpg_roles:
              - id: healer
                combat_function: "Restores allies"
                stat_affinity: [wisdom]
              - id: tank
                combat_function: "Absorbs damage"
                stat_affinity: [strength]
            npc_roles:
              - id: mentor
                narrative_function: "Guides protagonist"
        "#,
        )
        .unwrap()
    }

    fn test_constraints() -> ArchetypeConstraints {
        serde_yaml::from_str(
            r#"
            valid_pairings:
              common:
                - [sage, healer]
                - [hero, tank]
              uncommon: []
              rare: []
              forbidden: []
            genre_flavor:
              jungian: {}
              rpg_roles:
                healer:
                  fallback_name: "Hedge Healer"
                tank:
                  fallback_name: "Shield-Bearer"
            npc_roles_available: [mentor]
        "#,
        )
        .unwrap()
    }

    fn test_funnels() -> ArchetypeFunnels {
        serde_yaml::from_str(
            r#"
            funnels:
              - name: Thornwall Mender
                absorbs:
                  - [sage, healer]
                faction: Thornwall Convocation
                lore: "Itinerant healers"
                cultural_status: respected
            additional_constraints:
              forbidden: []
        "#,
        )
        .unwrap()
    }

    #[test]
    fn resolves_via_world_funnel() {
        let base = test_base();
        let constraints = test_constraints();
        let funnels = test_funnels();

        let result = resolve_archetype(
            "sage",
            "healer",
            &base,
            &constraints,
            Some(&funnels),
            "heavy_metal",
            Some("evropi"),
        )
        .unwrap();

        assert_eq!(result.resolved.name, "Thornwall Mender");
        assert_eq!(
            result.resolved.faction.as_deref(),
            Some("Thornwall Convocation")
        );
        assert!(result.resolved.lore.contains("Itinerant"));
        assert_eq!(result.source, ResolutionSource::WorldFunnel);
    }

    #[test]
    fn falls_back_to_genre() {
        let base = test_base();
        let constraints = test_constraints();

        let result = resolve_archetype(
            "hero",
            "tank",
            &base,
            &constraints,
            None,
            "heavy_metal",
            None,
        )
        .unwrap();

        assert_eq!(result.resolved.name, "Shield-Bearer");
        assert!(result.resolved.faction.is_none());
        assert_eq!(result.source, ResolutionSource::GenreFallback);
    }

    #[test]
    fn rejects_forbidden_pairing() {
        let base = test_base();
        let constraints: ArchetypeConstraints = serde_yaml::from_str(
            r#"
            valid_pairings:
              common: []
              uncommon: []
              rare: []
              forbidden:
                - [sage, tank]
            genre_flavor:
              jungian: {}
              rpg_roles: {}
            npc_roles_available: []
        "#,
        )
        .unwrap();

        let result = resolve_archetype(
            "sage",
            "tank",
            &base,
            &constraints,
            None,
            "heavy_metal",
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn rejects_unknown_axis_value() {
        let base = test_base();
        let constraints = test_constraints();

        let result = resolve_archetype(
            "nonexistent",
            "healer",
            &base,
            &constraints,
            None,
            "heavy_metal",
            None,
        );
        assert!(result.is_err());
    }
}
