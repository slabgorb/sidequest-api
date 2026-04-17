//! Story 38-8: Extend-and-return rule for sealed-letter dogfight combat.
//!
//! After a sealed-letter turn resolves with no hit (neither actor has
//! `gun_solution: true` in the resolved cell) and at least one actor's
//! post-resolution descriptor has `closure: opening_fast`, the engagement
//! has broken apart. The extend-and-return rule resets both actors to the
//! merge starting state while preserving energy, creating the 3-exchange
//! duel arc from the paper playtest.
//!
//! Acceptance criteria:
//!   - AC-1: Reset triggers on correct conditions (no hit + opening_fast).
//!   - AC-2: Reset preserves energy, clears geometric fields to merge state.
//!   - AC-3: Rule is documented for paper playtest consumption.
//!
//! All tests are expected to FAIL (RED state) until Dev implements the
//! post-resolution reset logic in `resolve_sealed_letter_lookup`.

use sidequest_game::encounter::{
    EncounterActor, EncounterMetric, MetricDirection, StructuredEncounter,
};
use sidequest_genre::InteractionTable;
use sidequest_server::resolve_sealed_letter_lookup;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// Fixtures
// ═══════════════════════════════════════════════════════════

/// Interaction table with cells that exercise extend-and-return conditions:
///
/// - `[bank, kill_rotation]` → evasive slips. Red: closure=opening, no shot.
///    Blue: closure=opening_fast, no shot. → TRIGGERS extend-and-return
///    (opening_fast + no hit)
///
/// - `[straight, straight]` → clean merge. Both: closure=opening, no shot.
///    → Does NOT trigger (opening, not opening_fast)
///
/// - `[loop, straight]` → offense scores. Red: gun_solution=true,
///    closure=opening. → Does NOT trigger (hit landed)
///
/// - `[kill_rotation, kill_rotation]` → mutual knife fight. Both:
///    gun_solution=true, closure=stable. → Does NOT trigger (hits landed)
fn extend_return_table_yaml() -> &'static str {
    r#"
version: "0.1.0"
starting_state: merge
maneuvers_consumed: [straight, bank, loop, kill_rotation]
cells:
  - pair: [bank, kill_rotation]
    name: "Red banks past the back-shot"
    shape: "evasive vs offense — evasive slips"
    red_view:
      target_bearing: "06"
      target_range: close
      target_aspect: head_on
      closure: opening
      gun_solution: false
    blue_view:
      target_bearing: "12"
      target_range: close
      target_aspect: crossing
      closure: opening_fast
      gun_solution: false
    narration_hint: "Red breaks, Blue's flip finds empty space."

  - pair: [straight, straight]
    name: "Clean merge"
    shape: "passive vs passive"
    red_view:
      target_bearing: "06"
      target_range: close
      target_aspect: tail_on
      closure: opening
      gun_solution: false
    blue_view:
      target_bearing: "06"
      target_range: close
      target_aspect: tail_on
      closure: opening
      gun_solution: false
    narration_hint: "Both ships rip past each other. No shots."

  - pair: [loop, straight]
    name: "Red reverses onto Blue's six"
    shape: "offense vs passive — offense scores"
    red_view:
      target_bearing: "12"
      target_range: close
      target_aspect: tail_on
      closure: opening
      gun_solution: true
      hit_severity: clean
    blue_view:
      target_bearing: "06"
      target_range: close
      target_aspect: head_on
      closure: opening
      gun_solution: false
    narration_hint: "Red loops and catches Blue's six."

  - pair: [kill_rotation, kill_rotation]
    name: "Drift-through knife fight"
    shape: "offense vs offense — mutual kill risk"
    red_view:
      target_bearing: "12"
      target_range: close
      target_aspect: head_on
      closure: stable
      gun_solution: true
      hit_severity: graze
    blue_view:
      target_bearing: "12"
      target_range: close
      target_aspect: head_on
      closure: stable
      gun_solution: true
      hit_severity: graze
    narration_hint: "Both ships drift through, guns trained."
"#
}

/// Build encounter with pre-populated energy on both actors.
fn encounter_with_energy(red_energy: i64, blue_energy: i64) -> StructuredEncounter {
    let mut red_state = HashMap::new();
    red_state.insert("viewer_energy".to_string(), serde_json::json!(red_energy));

    let mut blue_state = HashMap::new();
    blue_state.insert("viewer_energy".to_string(), serde_json::json!(blue_energy));

    StructuredEncounter {
        encounter_type: "dogfight".to_string(),
        metric: EncounterMetric {
            name: "advantage".to_string(),
            current: 0,
            starting: 0,
            direction: MetricDirection::Ascending,
            threshold_high: Some(3),
            threshold_low: Some(-3),
        },
        beat: 0,
        structured_phase: None,
        secondary_stats: None,
        actors: vec![
            EncounterActor {
                name: "Maverick".to_string(),
                role: "red".to_string(),
                per_actor_state: red_state,
            },
            EncounterActor {
                name: "Viper".to_string(),
                role: "blue".to_string(),
                per_actor_state: blue_state,
            },
        ],
        outcome: None,
        resolved: false,
        mood_override: None,
        narrator_hints: vec![],
    }
}

fn committed(red: &str, blue: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("red".to_string(), red.to_string());
    m.insert("blue".to_string(), blue.to_string());
    m
}

fn parse_table() -> InteractionTable {
    serde_yaml::from_str(extend_return_table_yaml())
        .expect("test fixture must parse")
}

/// Helper to read a string field from an actor's per_actor_state.
fn actor_state_str(encounter: &StructuredEncounter, role: &str, key: &str) -> Option<String> {
    encounter
        .actors
        .iter()
        .find(|a| a.role == role)
        .and_then(|a| a.per_actor_state.get(key))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Helper to read an i64 field from an actor's per_actor_state.
fn actor_state_i64(encounter: &StructuredEncounter, role: &str, key: &str) -> Option<i64> {
    encounter
        .actors
        .iter()
        .find(|a| a.role == role)
        .and_then(|a| a.per_actor_state.get(key))
        .and_then(|v| v.as_i64())
}

/// Helper to read a bool field from an actor's per_actor_state.
fn actor_state_bool(encounter: &StructuredEncounter, role: &str, key: &str) -> Option<bool> {
    encounter
        .actors
        .iter()
        .find(|a| a.role == role)
        .and_then(|a| a.per_actor_state.get(key))
        .and_then(|v| v.as_bool())
}

// Merge starting state values (from descriptor_schema.yaml):
const MERGE_BEARING: &str = "12";
const MERGE_RANGE: &str = "close";
const MERGE_ASPECT: &str = "head_on";
const MERGE_CLOSURE: &str = "closing_fast";

// ═══════════════════════════════════════════════════════════
// AC-1: Reset triggers on correct conditions
// ═══════════════════════════════════════════════════════════

#[test]
fn extend_return_triggers_on_opening_fast_no_hit() {
    // [bank, kill_rotation] → Blue has opening_fast, neither has gun_solution.
    // Extend-and-return MUST fire: both actors reset to merge geometry.
    let table = parse_table();
    let mut encounter = encounter_with_energy(45, 30);
    let commits = committed("bank", "kill_rotation");

    let _outcome = resolve_sealed_letter_lookup(&mut encounter, &commits, &table)
        .expect("resolution must succeed");

    // After extend-and-return, both actors should have merge starting geometry.
    assert_eq!(
        actor_state_str(&encounter, "red", "target_bearing").as_deref(),
        Some(MERGE_BEARING),
        "red target_bearing must reset to merge starting state after extend-and-return"
    );
    assert_eq!(
        actor_state_str(&encounter, "red", "closure").as_deref(),
        Some(MERGE_CLOSURE),
        "red closure must reset to closing_fast after extend-and-return"
    );
    assert_eq!(
        actor_state_str(&encounter, "blue", "target_bearing").as_deref(),
        Some(MERGE_BEARING),
        "blue target_bearing must reset to merge starting state"
    );
    assert_eq!(
        actor_state_str(&encounter, "blue", "closure").as_deref(),
        Some(MERGE_CLOSURE),
        "blue closure must reset to closing_fast"
    );
    assert_eq!(
        actor_state_bool(&encounter, "red", "gun_solution"),
        Some(false),
        "red gun_solution must be false after reset — no free shots"
    );
    assert_eq!(
        actor_state_bool(&encounter, "blue", "gun_solution"),
        Some(false),
        "blue gun_solution must be false after reset"
    );
}

#[test]
fn extend_return_does_not_trigger_on_opening_without_fast() {
    // [straight, straight] → both at closure: opening (NOT opening_fast).
    // Extend-and-return must NOT fire.
    let table = parse_table();
    let mut encounter = encounter_with_energy(60, 60);
    let commits = committed("straight", "straight");

    let _outcome = resolve_sealed_letter_lookup(&mut encounter, &commits, &table)
        .expect("resolution must succeed");

    // Closure should remain as the cell delta set it — "opening", not reset to "closing_fast".
    assert_eq!(
        actor_state_str(&encounter, "red", "closure").as_deref(),
        Some("opening"),
        "closure must stay 'opening' — extend-and-return should NOT fire without opening_fast"
    );
    // Bearing should be from the cell (06 for straight/straight), not reset to 12.
    assert_eq!(
        actor_state_str(&encounter, "red", "target_bearing").as_deref(),
        Some("06"),
        "target_bearing must stay as cell delta set it, not reset to merge"
    );
}

#[test]
fn extend_return_does_not_trigger_when_hit_landed() {
    // [loop, straight] → Red has gun_solution: true (hit landed).
    // Even if closure were opening_fast, the hit means no reset.
    let table = parse_table();
    let mut encounter = encounter_with_energy(30, 50);
    let commits = committed("loop", "straight");

    let _outcome = resolve_sealed_letter_lookup(&mut encounter, &commits, &table)
        .expect("resolution must succeed");

    // Red should have gun_solution: true from the cell — not reset.
    assert_eq!(
        actor_state_bool(&encounter, "red", "gun_solution"),
        Some(true),
        "gun_solution must remain true — a hit was scored, no extend-and-return"
    );
    // Bearing should be from the cell, not merge.
    assert_eq!(
        actor_state_str(&encounter, "red", "target_bearing").as_deref(),
        Some("12"),
        "target_bearing stays as cell set it when hit landed"
    );
}

#[test]
fn extend_return_does_not_trigger_on_mutual_hits() {
    // [kill_rotation, kill_rotation] → both gun_solution: true, closure: stable.
    // Hits landed on both sides — no reset.
    let table = parse_table();
    let mut encounter = encounter_with_energy(20, 20);
    let commits = committed("kill_rotation", "kill_rotation");

    let _outcome = resolve_sealed_letter_lookup(&mut encounter, &commits, &table)
        .expect("resolution must succeed");

    assert_eq!(
        actor_state_str(&encounter, "red", "closure").as_deref(),
        Some("stable"),
        "closure must stay 'stable' — both pilots hit, no extend-and-return"
    );
    assert_eq!(
        actor_state_bool(&encounter, "red", "gun_solution"),
        Some(true),
        "gun_solution stays true — hit was mutual"
    );
}

// ═══════════════════════════════════════════════════════════
// AC-2: Energy preservation across reset
// ═══════════════════════════════════════════════════════════

#[test]
fn extend_return_preserves_energy_on_reset() {
    // [bank, kill_rotation] triggers extend-and-return.
    // Red started with 45 energy, Blue with 30.
    // After reset, energy must carry over — NOT reset to 60.
    let table = parse_table();
    let mut encounter = encounter_with_energy(45, 30);
    let commits = committed("bank", "kill_rotation");

    let _outcome = resolve_sealed_letter_lookup(&mut encounter, &commits, &table)
        .expect("resolution must succeed");

    assert_eq!(
        actor_state_i64(&encounter, "red", "viewer_energy"),
        Some(45),
        "red energy must carry over after extend-and-return, not reset to 60"
    );
    assert_eq!(
        actor_state_i64(&encounter, "blue", "viewer_energy"),
        Some(30),
        "blue energy must carry over after extend-and-return"
    );
}

#[test]
fn extend_return_resets_all_geometric_fields() {
    // Verify ALL geometric fields reset, not just closure.
    let table = parse_table();
    let mut encounter = encounter_with_energy(45, 30);
    let commits = committed("bank", "kill_rotation");

    let _outcome = resolve_sealed_letter_lookup(&mut encounter, &commits, &table)
        .expect("resolution must succeed");

    // Red geometric fields must all be merge starting state.
    assert_eq!(actor_state_str(&encounter, "red", "target_bearing").as_deref(), Some(MERGE_BEARING));
    assert_eq!(actor_state_str(&encounter, "red", "target_range").as_deref(), Some(MERGE_RANGE));
    assert_eq!(actor_state_str(&encounter, "red", "target_aspect").as_deref(), Some(MERGE_ASPECT));
    assert_eq!(actor_state_str(&encounter, "red", "closure").as_deref(), Some(MERGE_CLOSURE));
    assert_eq!(actor_state_bool(&encounter, "red", "gun_solution"), Some(false));

    // Blue geometric fields must all be merge starting state.
    assert_eq!(actor_state_str(&encounter, "blue", "target_bearing").as_deref(), Some(MERGE_BEARING));
    assert_eq!(actor_state_str(&encounter, "blue", "target_range").as_deref(), Some(MERGE_RANGE));
    assert_eq!(actor_state_str(&encounter, "blue", "target_aspect").as_deref(), Some(MERGE_ASPECT));
    assert_eq!(actor_state_str(&encounter, "blue", "closure").as_deref(), Some(MERGE_CLOSURE));
    assert_eq!(actor_state_bool(&encounter, "blue", "gun_solution"), Some(false));
}

// ═══════════════════════════════════════════════════════════
// Wiring test — extend-and-return emits OTEL span
// ═══════════════════════════════════════════════════════════
// Per CLAUDE.md: the GM panel is the lie detector. The extend-and-return
// reset must be observable in OTEL so the GM knows the engine reset the
// engagement, not the narrator improvising.

#[test]
fn extend_return_emits_otel_span_on_reset() {
    use sidequest_telemetry::{init_global_channel, subscribe_global, WatcherEvent};

    let _ = init_global_channel();
    let mut rx = subscribe_global().expect("telemetry channel must initialize");
    while rx.try_recv().is_ok() {} // drain stale

    let table = parse_table();
    let mut encounter = encounter_with_energy(45, 30);
    let commits = committed("bank", "kill_rotation");

    let _outcome = resolve_sealed_letter_lookup(&mut encounter, &commits, &table)
        .expect("resolution must succeed");

    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }

    let reset_events: Vec<&WatcherEvent> = events
        .iter()
        .filter(|e| {
            e.component == "encounter"
                && e.fields
                    .get("event")
                    .and_then(serde_json::Value::as_str)
                    .map(|s| s.contains("extend_and_return"))
                    .unwrap_or(false)
        })
        .collect();

    assert_eq!(
        reset_events.len(),
        1,
        "extend-and-return must emit exactly one OTEL span so the GM panel can \
         verify the reset happened. Events seen: {:?}",
        events
            .iter()
            .filter_map(|e| e.fields.get("event").and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
    );
}
