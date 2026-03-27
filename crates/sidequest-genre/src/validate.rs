//! Two-phase validation: cross-reference checking.
//!
//! Phase 1 (serde deserialization) catches structural errors via `deny_unknown_fields`.
//! Phase 2 (this module) catches semantic errors — dangling references between
//! types that serde can't check.
//!
//! All validation methods collect errors instead of failing on first,
//! so users get a complete report of all issues at once.

use crate::error::{GenreError, ValidationErrors};
use crate::models::GenrePack;
use crate::util::slugify;
use std::collections::HashSet;

impl GenrePack {
    /// Validate cross-references within the loaded genre pack.
    ///
    /// Checks:
    /// - Achievement `trope_id` references an existing trope (genre or world-level)
    /// - Cartography region `adjacent` entries reference existing region slugs
    /// - Cartography route `from_id`/`to_id` reference existing region slugs
    /// - Scenario clue `implicates` entries reference existing suspect IDs
    ///
    /// Returns all errors found, not just the first.
    pub fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        self.validate_achievements(&mut errors);
        self.validate_cartography(&mut errors);
        self.validate_scenarios(&mut errors);
        errors.into_result()
    }

    fn validate_achievements(&self, errors: &mut ValidationErrors) {
        if self.achievements.is_empty() {
            return;
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
                errors.push(GenreError::ValidationError {
                    message: format!(
                        "achievement '{}' references trope_id '{}' which does not exist",
                        achievement.id, achievement.trope_id
                    ),
                });
            }
        }
    }

    fn validate_cartography(&self, errors: &mut ValidationErrors) {
        for (world_slug, world) in &self.worlds {
            let region_slugs: HashSet<&str> = world
                .cartography
                .regions
                .keys()
                .map(|s| s.as_str())
                .collect();

            // Check starting_region references an existing region
            if !world.cartography.starting_region.is_empty()
                && !region_slugs.contains(world.cartography.starting_region.as_str())
            {
                errors.push(GenreError::ValidationError {
                    message: format!(
                        "world '{world_slug}' has starting_region '{}' \
                         which does not exist",
                        world.cartography.starting_region
                    ),
                });
            }

            // Check adjacent references
            for (slug, region) in &world.cartography.regions {
                for adj in &region.adjacent {
                    if !region_slugs.contains(adj.as_str()) {
                        errors.push(GenreError::ValidationError {
                            message: format!(
                                "region '{slug}' in world '{world_slug}' has adjacent '{adj}' \
                                 which does not exist"
                            ),
                        });
                    }
                }
            }

            // Check route references (point-to-point format only)
            for route in &world.cartography.routes {
                if let Some(from) = &route.from_id {
                    if !region_slugs.contains(from.as_str()) {
                        errors.push(GenreError::ValidationError {
                            message: format!(
                                "route '{}' in world '{world_slug}' has from_id '{}' \
                                 which does not exist",
                                route.name, from
                            ),
                        });
                    }
                }
                if let Some(to) = &route.to_id {
                    if !region_slugs.contains(to.as_str()) {
                        errors.push(GenreError::ValidationError {
                            message: format!(
                                "route '{}' in world '{world_slug}' has to_id '{}' \
                                 which does not exist",
                                route.name, to
                            ),
                        });
                    }
                }
                // Waypoint format validation
                for wp in &route.waypoints {
                    if !region_slugs.contains(wp.as_str()) {
                        errors.push(GenreError::ValidationError {
                            message: format!(
                                "route '{}' in world '{world_slug}' has waypoint '{}' \
                                 which does not exist",
                                route.name, wp
                            ),
                        });
                    }
                }
            }
        }
    }

    fn validate_scenarios(&self, errors: &mut ValidationErrors) {
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
                        errors.push(GenreError::ValidationError {
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
    }
}
