//! Fixture file schema.
//!
//! A fixture is a YAML file that describes a minimal snapshot-worthy game
//! state. The character block is an opaque `serde_json::Value` so that it
//! deserializes directly into `sidequest_game::Character` via `serde_json`
//! at hydration time — this keeps the fixture crate decoupled from the
//! `Character` struct's internal layout (`#[serde(flatten)] core:
//! CreatureCore` etc.) and lets `NonBlankString` validation fire against
//! the raw JSON.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub name: String,
    pub genre: String,
    pub world: String,
    #[serde(default)]
    pub description: String,

    /// Player name used as the save-file path segment and echoed back to the
    /// UI so `dispatch_connect` picks up the matching save on WS connect.
    pub player_name: String,

    /// Opaque character block. Must deserialize into `sidequest_game::Character`
    /// via `serde_json::from_value`. Unknown fields are rejected by the
    /// `Character` struct's own serde rules.
    pub character: serde_json::Value,

    #[serde(default)]
    pub location: String,

    /// Optional turn counter. Maps to `TurnManager.interaction`.
    #[serde(default)]
    pub turn: u32,

    #[serde(default)]
    pub npcs: Vec<FixtureNpc>,

    pub encounter: FixtureEncounter,

    /// Optional starting resources (luck, humanity, fuel, etc.).
    #[serde(default)]
    pub resources: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureNpc {
    pub name: String,
    #[serde(default)]
    pub role: String,
    /// Integer disposition value (see `sidequest_game::Disposition`). Defaults
    /// to 0 (neutral).
    #[serde(default)]
    pub disposition: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureEncounter {
    /// Confrontation type key — must resolve to a `ConfrontationDef` in the
    /// loaded genre pack's `rules.yaml`. Beats, metrics, and thresholds all
    /// come from the def; nothing is hand-authored in the fixture.
    #[serde(rename = "type")]
    pub confrontation_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_fixture_round_trips() {
        let yaml = r#"
name: Test
genre: spaghetti_western
world: dust_and_lead
player_name: test-player
character:
  name: Dusty
encounter:
  type: poker
"#;
        let f: Fixture = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(f.genre, "spaghetti_western");
        assert_eq!(f.encounter.confrontation_type, "poker");
    }

    #[test]
    fn unknown_top_level_field_is_rejected() {
        let yaml = r#"
name: Test
genre: spaghetti_western
world: dust_and_lead
player_name: test-player
character:
  name: Dusty
encounter:
  type: poker
bogus_field: should_fail
"#;
        let result: Result<Fixture, _> = serde_yaml::from_str(yaml);
        assert!(
            result.is_err(),
            "deny_unknown_fields must reject bogus_field"
        );
    }
}
