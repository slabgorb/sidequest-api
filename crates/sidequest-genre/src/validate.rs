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
        self.validate_room_graph(&mut errors);
        self.validate_world_graph(&mut errors);
        self.validate_scenarios(&mut errors);
        self.validate_confrontations(&mut errors);
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

            // Check starting_region references an existing region (Region mode only)
            // RoomGraph mode validates starting_region in validate_room_graph
            if world.cartography.navigation_mode == crate::models::NavigationMode::Region {
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

    fn validate_confrontations(&self, errors: &mut ValidationErrors) {
        if self.rules.confrontations.is_empty() {
            return;
        }

        // Collect valid ability score names (uppercased for comparison)
        let ability_scores: HashSet<String> = self
            .rules
            .ability_score_names
            .iter()
            .map(|s| s.to_uppercase())
            .collect();

        // Collect all confrontation type IDs for escalates_to validation.
        // Include built-in engine encounter types (combat, chase) that exist
        // as StructuredEncounter presets even without YAML declarations.
        let mut confrontation_types: HashSet<&str> = self
            .rules
            .confrontations
            .iter()
            .map(|c| c.confrontation_type.as_str())
            .collect();
        confrontation_types.insert("combat");
        confrontation_types.insert("chase");

        for confrontation in &self.rules.confrontations {
            // Validate beat stat_check references
            for beat in &confrontation.beats {
                if !ability_scores.contains(&beat.stat_check.to_uppercase()) {
                    errors.push(GenreError::ValidationError {
                        message: format!(
                            "confrontation '{}' beat '{}' has stat_check '{}' \
                             which is not a declared ability score (valid: {:?})",
                            confrontation.confrontation_type,
                            beat.id,
                            beat.stat_check,
                            self.rules.ability_score_names
                        ),
                    });
                }
            }

            // Validate escalates_to references
            if let Some(ref target) = confrontation.escalates_to {
                if !confrontation_types.contains(target.as_str()) {
                    errors.push(GenreError::ValidationError {
                        message: format!(
                            "confrontation '{}' escalates_to '{}' \
                             which is not a declared confrontation type",
                            confrontation.confrontation_type, target
                        ),
                    });
                }
            }
        }
    }

    fn validate_room_graph(&self, errors: &mut ValidationErrors) {
        use crate::models::NavigationMode;
        use std::collections::VecDeque;

        for (world_slug, world) in &self.worlds {
            // Only validate room graph rules when navigation_mode is RoomGraph
            if world.cartography.navigation_mode != NavigationMode::RoomGraph {
                continue;
            }

            let rooms = match world.cartography.rooms.as_ref() {
                Some(r) => r,
                None => continue, // No rooms to validate
            };

            // Check for duplicate room IDs
            let mut seen_ids: HashSet<&str> = HashSet::new();
            for room in rooms {
                if !seen_ids.insert(room.id.as_str()) {
                    errors.push(GenreError::ValidationError {
                        message: format!(
                            "world '{world_slug}' has duplicate room ID '{}'",
                            room.id
                        ),
                    });
                }
            }

            let room_ids: HashSet<&str> = rooms.iter().map(|r| r.id.as_str()).collect();

            // Check starting_region references a valid room ID
            if !world.cartography.starting_region.is_empty()
                && !room_ids.contains(world.cartography.starting_region.as_str())
            {
                errors.push(GenreError::ValidationError {
                    message: format!(
                        "world '{world_slug}' has starting_region '{}' \
                         which is not a valid room ID",
                        world.cartography.starting_region
                    ),
                });
            }

            // Check all exit targets reference existing rooms
            for room in rooms {
                for exit in &room.exits {
                    if !room_ids.contains(exit.target()) {
                        errors.push(GenreError::ValidationError {
                            message: format!(
                                "room '{}' in world '{world_slug}' has exit to '{}' \
                                 which is not a valid room ID",
                                room.id,
                                exit.target()
                            ),
                        });
                    }
                }
            }

            // Check bidirectional exits — only exits where requires_reverse() is true
            for room in rooms {
                for exit in &room.exits {
                    if !exit.requires_reverse() {
                        continue; // Chutes don't require a return path
                    }
                    // Check that the target room has at least one exit back to this room
                    let has_return = rooms
                        .iter()
                        .find(|r| r.id == exit.target())
                        .map(|target_room| {
                            target_room
                                .exits
                                .iter()
                                .any(|e| e.target() == room.id)
                        })
                        .unwrap_or(false);

                    if !has_return {
                        errors.push(GenreError::ValidationError {
                            message: format!(
                                "room '{}' in world '{world_slug}' has non-chute exit to '{}' \
                                 but '{}' has no exit back to '{}'",
                                room.id,
                                exit.target(),
                                exit.target(),
                                room.id
                            ),
                        });
                    }
                }
            }

            // Require exactly one room with room_type "entrance"
            let entrance_rooms: Vec<&str> = rooms
                .iter()
                .filter(|r| r.room_type == "entrance")
                .map(|r| r.id.as_str())
                .collect();
            if entrance_rooms.is_empty() {
                errors.push(GenreError::ValidationError {
                    message: format!(
                        "world '{world_slug}' has no room with room_type 'entrance'"
                    ),
                });
            } else if entrance_rooms.len() > 1 {
                errors.push(GenreError::ValidationError {
                    message: format!(
                        "world '{world_slug}' has multiple entrance rooms: {}",
                        entrance_rooms.join(", ")
                    ),
                });
            }

            // Reject orphaned rooms unreachable from entrance (BFS)
            if let Some(entrance_id) = entrance_rooms.first() {
                let mut visited: HashSet<&str> = HashSet::new();
                let mut queue: VecDeque<&str> = VecDeque::new();
                queue.push_back(entrance_id);
                visited.insert(entrance_id);

                while let Some(current) = queue.pop_front() {
                    if let Some(room) = rooms.iter().find(|r| r.id == current) {
                        for exit in &room.exits {
                            if visited.insert(exit.target()) {
                                queue.push_back(exit.target());
                            }
                        }
                    }
                }

                for room in rooms {
                    if !visited.contains(room.id.as_str()) {
                        errors.push(GenreError::ValidationError {
                            message: format!(
                                "room '{}' in world '{world_slug}' is unreachable from \
                                 entrance (orphan)",
                                room.id
                            ),
                        });
                    }
                }
            }
        }
    }

    fn validate_world_graph(&self, errors: &mut ValidationErrors) {
        use crate::models::NavigationMode;

        for (world_slug, world) in &self.worlds {
            if world.cartography.navigation_mode != NavigationMode::Hierarchical {
                continue;
            }

            let world_graph = match world.cartography.world_graph.as_ref() {
                Some(wg) => wg,
                None => continue,
            };

            // Collect node IDs and check for duplicates
            let mut seen_ids: HashSet<&str> = HashSet::new();
            for node in &world_graph.nodes {
                if !seen_ids.insert(node.id.as_str()) {
                    errors.push(GenreError::ValidationError {
                        message: format!(
                            "world '{world_slug}' has duplicate world graph node ID '{}'",
                            node.id
                        ),
                    });
                }
            }

            let node_ids: HashSet<&str> = world_graph.nodes.iter().map(|n| n.id.as_str()).collect();

            // Check starting_region references a valid world graph node
            if !world.cartography.starting_region.is_empty()
                && !node_ids.contains(world.cartography.starting_region.as_str())
            {
                errors.push(GenreError::ValidationError {
                    message: format!(
                        "world '{world_slug}' has starting_region '{}' \
                         which is not a valid world graph node",
                        world.cartography.starting_region
                    ),
                });
            }

            // Check all edge endpoints reference existing nodes
            for edge in &world_graph.edges {
                if !node_ids.contains(edge.from.as_str()) {
                    errors.push(GenreError::ValidationError {
                        message: format!(
                            "world graph edge in world '{world_slug}' has from '{}' \
                             which is not a valid node",
                            edge.from
                        ),
                    });
                }
                if !node_ids.contains(edge.to.as_str()) {
                    errors.push(GenreError::ValidationError {
                        message: format!(
                            "world graph edge in world '{world_slug}' has to '{}' \
                             which is not a valid node",
                            edge.to
                        ),
                    });
                }
            }

            // Validate sub-graphs
            if let Some(sub_graphs) = world.cartography.sub_graphs.as_ref() {
                for (parent_id, sub_graph) in sub_graphs {
                    // Check parent exists in world graph
                    if !node_ids.contains(parent_id.as_str()) {
                        errors.push(GenreError::ValidationError {
                            message: format!(
                                "sub_graph '{parent_id}' in world '{world_slug}' \
                                 references nonexistent world graph node"
                            ),
                        });
                        continue;
                    }

                    // Collect sub-node IDs
                    let sub_node_ids: HashSet<&str> =
                        sub_graph.nodes.iter().map(|n| n.id.as_str()).collect();

                    // Check sub-graph edge endpoints
                    for edge in &sub_graph.edges {
                        if !sub_node_ids.contains(edge.from.as_str()) {
                            errors.push(GenreError::ValidationError {
                                message: format!(
                                    "sub_graph '{parent_id}' edge in world '{world_slug}' \
                                     has from '{}' which is not a valid sub-node",
                                    edge.from
                                ),
                            });
                        }
                        if !sub_node_ids.contains(edge.to.as_str()) {
                            errors.push(GenreError::ValidationError {
                                message: format!(
                                    "sub_graph '{parent_id}' edge in world '{world_slug}' \
                                     has to '{}' which is not a valid sub-node",
                                    edge.to
                                ),
                            });
                        }
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
