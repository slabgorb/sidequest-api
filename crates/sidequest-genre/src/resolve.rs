//! Trope inheritance resolution.
//!
//! World-level tropes can `extends` genre-level abstract tropes. This module
//! resolves the inheritance chain, merging parent fields into child tropes,
//! and detects cycles and missing parents.
//!
//! Python's resolver only handled one level of inheritance. This implementation
//! supports multi-level chains with proper cycle detection.

use crate::error::GenreError;
use crate::models::TropeDefinition;
use std::collections::{HashMap, HashSet};

/// Resolve trope inheritance by merging parent fields into child tropes.
///
/// - Genre-level tropes act as the parent pool.
/// - World-level tropes with `extends` inherit missing fields from their parent.
/// - Child fields override parent fields where both exist.
/// - Abstract tropes are not included in the result.
/// - Cycles in extends chains are detected and rejected.
pub fn resolve_trope_inheritance(
    genre_tropes: &[TropeDefinition],
    world_tropes: &[TropeDefinition],
) -> Result<Vec<TropeDefinition>, GenreError> {
    // Build parent lookup: normalized name slug → trope definition
    let mut parent_map: HashMap<String, &TropeDefinition> = HashMap::new();
    for trope in genre_tropes {
        let slug = slugify(trope.name.as_str());
        parent_map.insert(slug, trope);
    }
    // World tropes can also be parents (for multi-level chains)
    for trope in world_tropes {
        let slug = slugify(trope.name.as_str());
        parent_map.insert(slug, trope);
    }

    let mut resolved = Vec::new();

    for trope in world_tropes {
        if let Some(parent_slug) = &trope.extends {
            // Check for missing parent
            if !parent_map.contains_key(parent_slug) {
                return Err(GenreError::MissingParent {
                    trope: trope.name.as_str().to_string(),
                    parent: parent_slug.clone(),
                });
            }

            // Detect cycles
            let mut visited = HashSet::new();
            visited.insert(slugify(trope.name.as_str()));
            detect_cycle(parent_slug, &parent_map, &mut visited)?;

            // Merge: child overrides parent
            let parent = parent_map[parent_slug];
            let merged = merge_trope(parent, trope);
            resolved.push(merged);
        } else {
            // No extends — include as-is
            resolved.push(trope.clone());
        }
    }

    Ok(resolved)
}

/// Detect cycles in the extends chain starting from `current_slug`.
fn detect_cycle(
    current_slug: &str,
    parent_map: &HashMap<String, &TropeDefinition>,
    visited: &mut HashSet<String>,
) -> Result<(), GenreError> {
    if !visited.insert(current_slug.to_string()) {
        return Err(GenreError::CycleDetected {
            trope: current_slug.to_string(),
        });
    }

    if let Some(trope) = parent_map.get(current_slug) {
        if let Some(next_parent) = &trope.extends {
            detect_cycle(next_parent, parent_map, visited)?;
        }
    }

    Ok(())
}

/// Merge a child trope with its parent: child fields override parent fields.
fn merge_trope(parent: &TropeDefinition, child: &TropeDefinition) -> TropeDefinition {
    TropeDefinition {
        id: child.id.clone().or_else(|| parent.id.clone()),
        name: child.name.clone(),
        description: child
            .description
            .clone()
            .or_else(|| parent.description.clone()),
        // Child category overrides if non-empty, else inherit from parent
        category: if child.category.is_empty() {
            parent.category.clone()
        } else {
            child.category.clone()
        },
        triggers: if child.triggers.is_empty() {
            parent.triggers.clone()
        } else {
            child.triggers.clone()
        },
        narrative_hints: if child.narrative_hints.is_empty() {
            parent.narrative_hints.clone()
        } else {
            child.narrative_hints.clone()
        },
        tension_level: if child.tension_level == 0.0 {
            parent.tension_level
        } else {
            child.tension_level
        },
        resolution_hints: child
            .resolution_hints
            .clone()
            .or_else(|| parent.resolution_hints.clone()),
        resolution_patterns: child
            .resolution_patterns
            .clone()
            .or_else(|| parent.resolution_patterns.clone()),
        tags: if child.tags.is_empty() {
            parent.tags.clone()
        } else {
            child.tags.clone()
        },
        escalation: if child.escalation.is_empty() {
            parent.escalation.clone()
        } else {
            child.escalation.clone()
        },
        passive_progression: child
            .passive_progression
            .clone()
            .or_else(|| parent.passive_progression.clone()),
        // Resolved tropes are never abstract
        is_abstract: false,
        // Clear extends after resolution
        extends: None,
    }
}

/// Convert a trope name to a slug for lookup (lowercase, spaces → hyphens).
fn slugify(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}
