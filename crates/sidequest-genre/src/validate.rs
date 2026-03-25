//! Two-phase validation: cross-reference checking.
//!
//! Phase 1 (serde deserialization) catches structural errors via `deny_unknown_fields`.
//! Phase 2 (this module) catches semantic errors — dangling references between
//! types that serde can't check.

use crate::error::GenreError;
use crate::models::GenrePack;
use std::collections::HashSet;

impl GenrePack {
    /// Validate cross-references within the loaded genre pack.
    ///
    /// Checks:
    /// - Achievement `trope_id` references an existing trope (genre or world-level)
    /// - Cartography region `adjacent` entries reference existing region slugs
    /// - Cartography route `from_id`/`to_id` reference existing region slugs
    /// - Scenario clue `implicates` entries reference existing suspect IDs
    pub fn validate(&self) -> Result<(), GenreError> {
        self.validate_achievements()?;
        self.validate_cartography()?;
        self.validate_scenarios()?;
        Ok(())
    }

    fn validate_achievements(&self) -> Result<(), GenreError> {
        if self.achievements.is_empty() {
            return Ok(());
        }

        // Collect all trope IDs/names from genre-level and world-level tropes
        let mut trope_ids: HashSet<String> = HashSet::new();
        for trope in &self.tropes {
            if let Some(id) = &trope.id {
                trope_ids.insert(id.clone());
            }
            trope_ids.insert(slugify(trope.name.as_str()));
        }
        for world in self.worlds.values() {
            for trope in &world.tropes {
                if let Some(id) = &trope.id {
                    trope_ids.insert(id.clone());
                }
                trope_ids.insert(slugify(trope.name.as_str()));
            }
        }

        for achievement in &self.achievements {
            if !trope_ids.contains(&achievement.trope_id) {
                return Err(GenreError::ValidationError {
                    message: format!(
                        "achievement '{}' references trope_id '{}' which does not exist",
                        achievement.id, achievement.trope_id
                    ),
                });
            }
        }

        Ok(())
    }

    fn validate_cartography(&self) -> Result<(), GenreError> {
        for (world_slug, world) in &self.worlds {
            let region_slugs: HashSet<&str> = world
                .cartography
                .regions
                .keys()
                .map(|s| s.as_str())
                .collect();

            // Check adjacent references
            for (slug, region) in &world.cartography.regions {
                for adj in &region.adjacent {
                    if !region_slugs.contains(adj.as_str()) {
                        return Err(GenreError::ValidationError {
                            message: format!(
                                "region '{slug}' in world '{world_slug}' has adjacent '{adj}' \
                                 which does not exist"
                            ),
                        });
                    }
                }
            }

            // Check route references
            for route in &world.cartography.routes {
                if !region_slugs.contains(route.from_id.as_str()) {
                    return Err(GenreError::ValidationError {
                        message: format!(
                            "route '{}' in world '{world_slug}' has from_id '{}' \
                             which does not exist",
                            route.name, route.from_id
                        ),
                    });
                }
                if !region_slugs.contains(route.to_id.as_str()) {
                    return Err(GenreError::ValidationError {
                        message: format!(
                            "route '{}' in world '{world_slug}' has to_id '{}' \
                             which does not exist",
                            route.name, route.to_id
                        ),
                    });
                }
            }
        }

        Ok(())
    }

    fn validate_scenarios(&self) -> Result<(), GenreError> {
        for (scenario_slug, scenario) in &self.scenarios {
            // Collect suspect IDs
            let suspect_ids: HashSet<&str> = scenario
                .assignment_matrix
                .suspects
                .iter()
                .map(|s| s.id.as_str())
                .collect();

            // Check clue graph references
            for node in &scenario.clue_graph.nodes {
                for suspect_ref in &node.implicates {
                    if !suspect_ids.contains(suspect_ref.as_str()) {
                        return Err(GenreError::ValidationError {
                            message: format!(
                                "clue '{}' in scenario '{scenario_slug}' implicates '{}' \
                                 which is not a defined suspect",
                                node.id, suspect_ref
                            ),
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

/// Convert a name to a slug for lookup.
fn slugify(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}
