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
use crate::util::slugify;
use std::collections::{HashMap, HashSet};

/// Maximum depth for trope inheritance chains.
/// Prevents stack overflow from deeply nested (non-cyclic) extends chains (CWE-674).
const MAX_INHERITANCE_DEPTH: usize = 64;

/// Resolve trope inheritance by merging parent fields into child tropes.
///
/// - Genre-level tropes act as the parent pool (looked up by slugified name).
/// - World-level tropes with `extends` inherit missing fields from their parent.
/// - Child fields override parent fields where both exist.
/// - Only world tropes appear in the output; genre-level abstract tropes serve
///   as parents but are not emitted directly.
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
        if let Some(raw_parent_slug) = &trope.extends {
            // Normalize extends value to match the slug-keyed parent_map
            let parent_slug = slugify(raw_parent_slug);

            // Check for missing parent
            if !parent_map.contains_key(&parent_slug) {
                return Err(GenreError::MissingParent {
                    trope: trope.name.as_str().to_string(),
                    parent: raw_parent_slug.clone(),
                });
            }

            // Detect cycles (with depth limit)
            let mut visited = HashSet::new();
            visited.insert(slugify(trope.name.as_str()));
            detect_cycle(&parent_slug, &parent_map, &mut visited, 0)?;

            // Merge: child overrides parent
            let parent = parent_map[&parent_slug];
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
/// Also enforces a maximum depth to prevent stack overflow on deep non-cyclic chains.
fn detect_cycle(
    current_slug: &str,
    parent_map: &HashMap<String, &TropeDefinition>,
    visited: &mut HashSet<String>,
    depth: usize,
) -> Result<(), GenreError> {
    if depth > MAX_INHERITANCE_DEPTH {
        return Err(GenreError::ValidationError {
            message: format!(
                "trope inheritance chain exceeds maximum depth of {MAX_INHERITANCE_DEPTH}"
            ),
        });
    }

    if !visited.insert(current_slug.to_string()) {
        return Err(GenreError::CycleDetected {
            trope: current_slug.to_string(),
        });
    }

    if let Some(trope) = parent_map.get(current_slug) {
        if let Some(next_parent) = &trope.extends {
            let next_slug = slugify(next_parent);
            detect_cycle(&next_slug, parent_map, visited, depth + 1)?;
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
        tension_level: child.tension_level.or(parent.tension_level),
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
