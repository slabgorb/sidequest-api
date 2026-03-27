//! Patch legality checks — deterministic validation of state mutations.
//!
//! Story 3-3: First validation module in the cold-path validator.
//! Compares patches against game rules and emits `tracing::warn!` for violations.
//!
//! Each check function receives a `&TurnRecord` and returns a list of
//! `ValidationResult`s. The runner `run_legality_checks` aggregates all checks.

use sidequest_game::combatant::Combatant;

use crate::turn_record::TurnRecord;

/// Result of a single validation check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    /// Check passed — no issues found.
    Ok,
    /// Non-fatal heuristic concern (logged but not alarming).
    Warning(String),
    /// Rule violation detected (indicates a bug in agent output or patch application).
    Violation(String),
}

/// Check that no creature's HP exceeds their max_hp in snapshot_after.
pub fn check_hp_bounds(record: &TurnRecord) -> Vec<ValidationResult> {
    let mut results = Vec::new();

    for npc in &record.snapshot_after.npcs {
        if npc.core.hp > npc.core.max_hp {
            results.push(ValidationResult::Violation(format!(
                "NPC {}: HP {} exceeds max_hp {}",
                npc.core.name.as_str(),
                npc.core.hp,
                npc.core.max_hp
            )));
        }
    }

    for character in &record.snapshot_after.characters {
        if character.core.hp > character.core.max_hp {
            results.push(ValidationResult::Violation(format!(
                "{}: HP {} exceeds max_hp {}",
                character.core.name.as_str(),
                character.core.hp,
                character.core.max_hp
            )));
        }
    }

    results
}

/// Check that dead entities (hp == 0 or "dead" in statuses) did not act or gain HP.
pub fn check_dead_entity_actions(record: &TurnRecord) -> Vec<ValidationResult> {
    let mut results = Vec::new();

    for npc_before in &record.snapshot_before.npcs {
        let is_dead =
            !npc_before.is_alive() || npc_before.core.statuses.contains(&"dead".to_string());

        if !is_dead {
            continue;
        }

        let name = npc_before.core.name.as_str();

        if let Some(npc_after) = record
            .snapshot_after
            .npcs
            .iter()
            .find(|n| n.core.name.as_str() == name)
        {
            // Dead NPC gained HP
            if npc_after.core.hp > npc_before.core.hp {
                results.push(ValidationResult::Violation(format!(
                    "Dead NPC {} gained HP ({} -> {})",
                    name, npc_before.core.hp, npc_after.core.hp
                )));
            }

            // Dead NPC changed location
            if npc_after.location != npc_before.location {
                results.push(ValidationResult::Violation(format!(
                    "Dead NPC {} changed location",
                    name
                )));
            }
        }
    }

    results
}

/// Check that location transitions are to discovered regions.
pub fn check_location_validity(record: &TurnRecord) -> Vec<ValidationResult> {
    let before_region = &record.snapshot_before.current_region;
    let after_region = &record.snapshot_after.current_region;

    if before_region != after_region {
        if !record
            .snapshot_after
            .discovered_regions
            .contains(after_region)
        {
            return vec![ValidationResult::Violation(format!(
                "Region changed to '{}' which is not in discovered_regions",
                after_region
            ))];
        }
    }

    vec![]
}

/// Check that combat patches are only applied when combat is active.
///
/// Combat is considered active when round > 1 or the damage log is non-empty.
pub fn check_combat_coherence(record: &TurnRecord) -> Vec<ValidationResult> {
    let has_combat_patch = record
        .patches_applied
        .iter()
        .any(|p| p.patch_type == "combat");

    if !has_combat_patch {
        return vec![];
    }

    let combat = &record.snapshot_before.combat;
    let combat_active = combat.round() > 1 || !combat.damage_log().is_empty();

    if !combat_active {
        return vec![ValidationResult::Violation(
            "CombatPatch applied but combat is not active".into(),
        )];
    }

    vec![]
}

/// Check that chase patches are only applied when a chase is active.
pub fn check_chase_coherence(record: &TurnRecord) -> Vec<ValidationResult> {
    let has_chase_patch = record
        .patches_applied
        .iter()
        .any(|p| p.patch_type == "chase");

    if !has_chase_patch {
        return vec![];
    }

    if record.snapshot_before.chase.is_none() {
        return vec![ValidationResult::Violation(
            "ChasePatch applied but no chase is active".into(),
        )];
    }

    vec![]
}

/// Run all legality checks against a TurnRecord.
///
/// Each violation emits a `tracing::warn!` with `component="watcher"`
/// and `check="patch_legality"`.
///
/// Returns the aggregated list of all results (Ok, Warning, Violation).
pub fn run_legality_checks(record: &TurnRecord) -> Vec<ValidationResult> {
    let checks: Vec<fn(&TurnRecord) -> Vec<ValidationResult>> = vec![
        check_hp_bounds,
        check_dead_entity_actions,
        check_combat_coherence,
        check_chase_coherence,
        check_location_validity,
    ];

    let mut all_results = Vec::new();
    for check in checks {
        let results = check(record);
        for result in &results {
            match result {
                ValidationResult::Warning(msg) => {
                    tracing::warn!(
                        component = "watcher",
                        check = "patch_legality",
                        msg = %msg,
                    );
                }
                ValidationResult::Violation(msg) => {
                    tracing::warn!(
                        component = "watcher",
                        check = "patch_legality",
                        severity = "violation",
                        msg = %msg,
                    );
                }
                ValidationResult::Ok => {}
            }
        }
        all_results.extend(results);
    }

    all_results
}
