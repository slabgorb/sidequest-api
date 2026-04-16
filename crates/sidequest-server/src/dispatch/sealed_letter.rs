//! Sealed-letter lookup resolution handler (Story 38-5).
//!
//! Resolves simultaneous-commit encounters where two actors each commit a
//! maneuver privately, and the engine resolves via cross-product lookup in
//! an interaction table (ADR-077, Epic 38).
//!
//! The handler is synchronous — async commit-gathering from TurnBarrier
//! happens at the dispatch call site, which passes the resolved maneuvers
//! as a `HashMap<String, String>` keyed by actor role ("red" / "blue").

use std::collections::HashMap;

use sidequest_game::encounter::StructuredEncounter;
use sidequest_genre::InteractionTable;

use crate::{Severity, WatcherEventBuilder, WatcherEventType};

/// Outcome of a sealed-letter lookup resolution.
///
/// Carries the matched cell name and the committed maneuvers for
/// downstream consumers (narration, OTEL, etc.).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedLetterOutcome {
    /// Name of the matched interaction cell (e.g., "Clean break").
    pub cell_name: String,
    /// The maneuver committed by the red actor.
    pub red_maneuver: String,
    /// The maneuver committed by the blue actor.
    pub blue_maneuver: String,
}

/// Resolve a sealed-letter lookup turn.
///
/// Given committed maneuvers (keyed by actor role: "red" / "blue") and
/// an interaction table, looks up the cross-product cell, applies
/// `red_view` / `blue_view` descriptor deltas to each actor's
/// `per_actor_state`, and emits OTEL spans at each step.
///
/// # Errors
///
/// Returns `Err` if:
/// - The committed maneuvers are missing the "red" or "blue" key.
/// - No interaction cell matches the `(red, blue)` maneuver pair
///   (no silent fallback — project rule).
pub fn resolve_sealed_letter_lookup(
    encounter: &mut StructuredEncounter,
    commits: &HashMap<String, String>,
    table: &InteractionTable,
) -> Result<SealedLetterOutcome, String> {
    // Extract committed maneuvers.
    let red_maneuver = commits
        .get("red")
        .ok_or_else(|| "committed maneuvers missing 'red' key".to_string())?
        .clone();
    let blue_maneuver = commits
        .get("blue")
        .ok_or_else(|| "committed maneuvers missing 'blue' key".to_string())?
        .clone();

    // OTEL: commits gathered.
    WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
        .field("event", "encounter.sealed_letter.commits_gathered")
        .field("red_maneuver", &red_maneuver)
        .field("blue_maneuver", &blue_maneuver)
        .field("encounter_type", &encounter.encounter_type)
        .send();
    tracing::info!(
        red_maneuver = %red_maneuver,
        blue_maneuver = %blue_maneuver,
        encounter_type = %encounter.encounter_type,
        "encounter.sealed_letter.commits_gathered"
    );

    // Cell lookup: find the (red, blue) pair in the interaction table.
    let cell = table
        .cells
        .iter()
        .find(|c| c.pair.0 == red_maneuver && c.pair.1 == blue_maneuver);

    let cell = match cell {
        Some(c) => {
            // OTEL: cell lookup success.
            WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                .field("event", "encounter.sealed_letter.cell_lookup")
                .field("cell_name", &c.name)
                .field("red_maneuver", &red_maneuver)
                .field("blue_maneuver", &blue_maneuver)
                .field("encounter_type", &encounter.encounter_type)
                .send();
            tracing::info!(
                cell_name = %c.name,
                red_maneuver = %red_maneuver,
                blue_maneuver = %blue_maneuver,
                "encounter.sealed_letter.cell_lookup"
            );
            c
        }
        None => {
            // OTEL: cell not found — loud failure, no silent fallback.
            tracing::warn!(
                red_maneuver = %red_maneuver,
                blue_maneuver = %blue_maneuver,
                encounter_type = %encounter.encounter_type,
                "encounter.sealed_letter.cell_not_found — no matching cell in interaction table"
            );
            WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
                .field("event", "encounter.sealed_letter.cell_not_found")
                .field("red_maneuver", &red_maneuver)
                .field("blue_maneuver", &blue_maneuver)
                .field("encounter_type", &encounter.encounter_type)
                .severity(Severity::Warn)
                .send();
            return Err(format!(
                "no interaction cell for maneuver pair ({}, {}) in table",
                red_maneuver, blue_maneuver
            ));
        }
    };

    let cell_name = cell.name.clone();

    // Delta application: merge cell views into actors' per_actor_state.
    // red_view → actor with role "red", blue_view → actor with role "blue".
    // Merge (insert/overwrite keys), do NOT replace the entire HashMap.
    let red_applied = apply_view_deltas(encounter, "red", &cell.red_view);
    let blue_applied = apply_view_deltas(encounter, "blue", &cell.blue_view);

    // OTEL: deltas applied — only if at least one side actually applied.
    // If both views were non-Mapping (content error), the GM panel must NOT
    // see "deltas_applied" because that would be a lie.
    if red_applied || blue_applied {
        WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
            .field("event", "encounter.sealed_letter.deltas_applied")
            .field("cell_name", &cell_name)
            .field("red_maneuver", &red_maneuver)
            .field("blue_maneuver", &blue_maneuver)
            .field("red_applied", red_applied)
            .field("blue_applied", blue_applied)
            .field("encounter_type", &encounter.encounter_type)
            .send();
        tracing::info!(
            cell_name = %cell_name,
            red_applied,
            blue_applied,
            "encounter.sealed_letter.deltas_applied"
        );
    } else {
        tracing::warn!(
            cell_name = %cell_name,
            "encounter.sealed_letter.deltas_not_applied — both views were non-Mapping"
        );
        WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
            .field("event", "encounter.sealed_letter.deltas_not_applied")
            .field("cell_name", &cell_name)
            .field("red_maneuver", &red_maneuver)
            .field("blue_maneuver", &blue_maneuver)
            .field("encounter_type", &encounter.encounter_type)
            .severity(Severity::Warn)
            .send();
    }

    Ok(SealedLetterOutcome {
        cell_name,
        red_maneuver,
        blue_maneuver,
    })
}

/// Merge a `serde_yaml::Value` (expected to be a mapping) into the
/// specified actor's `per_actor_state`. Keys are inserted/overwritten;
/// existing keys not in the view are preserved.
///
/// Returns `true` if deltas were actually applied, `false` if the view
/// was not a Mapping or the actor was not found. The caller gates the
/// `deltas_applied` OTEL event on this return value.
fn apply_view_deltas(
    encounter: &mut StructuredEncounter,
    role: &str,
    view: &serde_yaml::Value,
) -> bool {
    let Some(actor) = encounter.actors.iter_mut().find(|a| a.role == role) else {
        tracing::warn!(
            role = %role,
            "sealed_letter: no actor with role '{}' — skipping delta application",
            role
        );
        return false;
    };

    match view {
        serde_yaml::Value::Mapping(map) => {
            for (key, value) in map {
                if let serde_yaml::Value::String(key_str) = key {
                    let json_value = yaml_value_to_json(value);
                    actor
                        .per_actor_state
                        .insert(key_str.clone(), json_value);
                } else {
                    tracing::warn!(
                        role = %role,
                        key = ?key,
                        "sealed_letter: non-string key in cell view — dropped"
                    );
                }
            }
            true
        }
        serde_yaml::Value::Null => {
            // Null view = legitimate "no state change for this actor".
            true
        }
        other => {
            // Non-Mapping, non-Null view is a content authoring error.
            tracing::warn!(
                role = %role,
                view_type = ?other,
                "sealed_letter: cell view is not a Mapping — content error, no deltas applied"
            );
            WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
                .field("event", "encounter.sealed_letter.invalid_view_type")
                .field("role", role)
                .severity(Severity::Warn)
                .send();
            false
        }
    }
}

/// Convert a `serde_yaml::Value` to a `serde_json::Value`.
///
/// Handles the types that appear in interaction table cell views:
/// strings, booleans, integers, floats, and nulls.
fn yaml_value_to_json(value: &serde_yaml::Value) -> serde_json::Value {
    match value {
        serde_yaml::Value::String(s) => serde_json::Value::String(s.clone()),
        serde_yaml::Value::Bool(b) => serde_json::Value::Bool(*b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::Value::Number(i.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::json!(f)
            } else {
                serde_json::Value::Null
            }
        }
        serde_yaml::Value::Null => serde_json::Value::Null,
        serde_yaml::Value::Sequence(seq) => {
            serde_json::Value::Array(seq.iter().map(yaml_value_to_json).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .filter_map(|(k, v)| {
                    if let serde_yaml::Value::String(ks) = k {
                        Some((ks.clone(), yaml_value_to_json(v)))
                    } else {
                        None
                    }
                })
                .collect();
            serde_json::Value::Object(obj)
        }
        serde_yaml::Value::Tagged(tagged) => yaml_value_to_json(&tagged.value),
    }
}
