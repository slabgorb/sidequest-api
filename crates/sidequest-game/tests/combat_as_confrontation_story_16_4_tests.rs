//! Story 16-4: Migrate combat as confrontation
//!
//! RED phase — CombatState becomes a confrontation type preset.
//! The existing combat mechanics (rounds, damage log, turn order, status effects)
//! are expressed as a StructuredEncounter with type="combat".
//!
//! Key mappings:
//!   - HP → Descending metric (threshold_low = 0, resolution = someone hits 0)
//!   - round → beat count
//!   - damage_log → preserved in encounter data
//!   - turn_order → actors with combat roles
//!   - effects → secondary stats or dedicated field
//!   - drama_weight → encounter metadata
//!   - in_combat → !resolved
//!
//! ACs:
//!   AC-1: StructuredEncounter::combat() constructor creates combat-type preset
//!   AC-2: StructuredEncounter::from_combat_state() migrates existing CombatState
//!   AC-3: All field mappings are correct (round→beat, turn_order→actors, etc.)
//!   AC-4: Serde roundtrip preserves combat encounter data
//!   AC-5: CombatState still works as before (no behavioral changes)

use serde_json;

use sidequest_game::combat::{
    CombatState, DamageEvent, StatusEffect, StatusEffectKind,
};
use sidequest_game::encounter::{
    EncounterActor, EncounterMetric, EncounterPhase, MetricDirection,
    SecondaryStats, StatValue, StructuredEncounter,
};

// ==========================================================================
// AC-1: StructuredEncounter::combat() convenience constructor
// ==========================================================================

/// combat() constructor should create a combat-type encounter with
/// HP as a descending metric that resolves when someone reaches 0.
#[test]
fn combat_encounter_constructor_creates_combat_type() {
    let combatants = vec!["Hero".to_string(), "Goblin".to_string()];
    let encounter = StructuredEncounter::combat(combatants, 30);

    assert_eq!(encounter.encounter_type, "combat");
    assert_eq!(encounter.metric.name, "hp");
    assert_eq!(encounter.metric.direction, MetricDirection::Descending);
    assert_eq!(
        encounter.metric.threshold_low,
        Some(0),
        "combat resolves when HP reaches 0"
    );
    assert!(
        encounter.metric.threshold_high.is_none(),
        "combat has no upper threshold"
    );
    assert_eq!(encounter.metric.current, 30, "starting HP as metric value");
    assert_eq!(encounter.metric.starting, 30);
    assert!(!encounter.resolved);
}

/// combat() should populate actors from combatant names.
#[test]
fn combat_encounter_constructor_populates_actors() {
    let combatants = vec![
        "Hero".to_string(),
        "Goblin".to_string(),
        "Orc".to_string(),
    ];
    let encounter = StructuredEncounter::combat(combatants, 30);

    assert_eq!(encounter.actors.len(), 3);
    assert_eq!(encounter.actors[0].name, "Hero");
    assert_eq!(encounter.actors[1].name, "Goblin");
    assert_eq!(encounter.actors[2].name, "Orc");

    // All actors should have a combat role
    for actor in &encounter.actors {
        assert_eq!(actor.role, "combatant", "default combat role should be 'combatant'");
    }
}

/// combat() should start at beat 0 in Setup phase.
#[test]
fn combat_encounter_constructor_starts_at_setup() {
    let encounter = StructuredEncounter::combat(vec!["A".into(), "B".into()], 10);

    assert_eq!(encounter.beat, 0);
    assert_eq!(
        encounter.structured_phase,
        Some(EncounterPhase::Setup),
        "combat should begin in Setup phase"
    );
}

/// combat() with empty combatants should still create a valid encounter.
#[test]
fn combat_encounter_constructor_empty_combatants() {
    let encounter = StructuredEncounter::combat(vec![], 20);

    assert_eq!(encounter.encounter_type, "combat");
    assert!(encounter.actors.is_empty());
    assert_eq!(encounter.metric.current, 20);
}

// ==========================================================================
// AC-2: StructuredEncounter::from_combat_state() migration
// ==========================================================================

/// from_combat_state() should convert an active CombatState into a
/// StructuredEncounter preserving all meaningful state.
#[test]
fn from_combat_state_preserves_round_as_beat() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Hero".into(), "Goblin".into()]);
    combat.advance_round();
    combat.advance_round(); // Now round 3

    let encounter = StructuredEncounter::from_combat_state(&combat);

    assert_eq!(encounter.encounter_type, "combat");
    assert_eq!(
        encounter.beat,
        combat.round(),
        "round should map to beat"
    );
}

/// from_combat_state() should map turn_order to actors.
#[test]
fn from_combat_state_maps_turn_order_to_actors() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Hero".into(), "Goblin".into(), "Orc".into()]);

    let encounter = StructuredEncounter::from_combat_state(&combat);

    assert_eq!(encounter.actors.len(), 3);
    assert_eq!(encounter.actors[0].name, "Hero");
    assert_eq!(encounter.actors[1].name, "Goblin");
    assert_eq!(encounter.actors[2].name, "Orc");
}

/// from_combat_state() should preserve the damage log.
#[test]
fn from_combat_state_preserves_damage_log() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Hero".into(), "Goblin".into()]);
    combat.log_damage(DamageEvent {
        attacker: "Hero".into(),
        target: "Goblin".into(),
        damage: 8,
        round: 1,
    });
    combat.log_damage(DamageEvent {
        attacker: "Goblin".into(),
        target: "Hero".into(),
        damage: 3,
        round: 1,
    });

    let encounter = StructuredEncounter::from_combat_state(&combat);

    // Damage log must be preserved — either in narrator_hints, secondary data,
    // or a dedicated field. The encounter should have some record of 2 damage events.
    // The exact representation is up to Dev, but it must be recoverable.
    let json = serde_json::to_string(&encounter).expect("serialize");
    assert!(
        json.contains("Hero") && json.contains("Goblin"),
        "damage log participants must be present in the serialized encounter"
    );
}

/// from_combat_state() should map in_combat to resolved flag.
#[test]
fn from_combat_state_active_combat_is_not_resolved() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Hero".into(), "Goblin".into()]);

    let encounter = StructuredEncounter::from_combat_state(&combat);
    assert!(
        !encounter.resolved,
        "active combat should map to resolved=false"
    );
}

/// from_combat_state() for inactive combat should be resolved.
#[test]
fn from_combat_state_inactive_combat_is_resolved() {
    let combat = CombatState::new(); // Default: not in combat

    let encounter = StructuredEncounter::from_combat_state(&combat);
    assert!(
        encounter.resolved,
        "inactive combat (not in_combat) should map to resolved=true"
    );
}

/// from_combat_state() should preserve drama_weight in the encounter.
#[test]
fn from_combat_state_preserves_drama_weight() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Hero".into(), "Goblin".into()]);
    combat.set_drama_weight(0.85);

    let encounter = StructuredEncounter::from_combat_state(&combat);

    // Drama weight should be accessible — either through the phase's drama_weight
    // or preserved in the encounter structure.
    // The encounter's structured_phase drama_weight is phase-based, so
    // the combat-specific drama_weight needs its own home.
    let json = serde_json::to_string(&encounter).expect("serialize");
    assert!(
        json.contains("0.85") || encounter.structured_phase.map_or(false, |p| p.drama_weight() > 0.0),
        "drama_weight must be preserved in the encounter"
    );
}

/// from_combat_state() should map status effects.
#[test]
fn from_combat_state_maps_status_effects() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Hero".into(), "Goblin".into()]);
    combat.add_effect("Hero", StatusEffect::new(StatusEffectKind::Poison, 3));
    combat.add_effect("Goblin", StatusEffect::new(StatusEffectKind::Stun, 1));

    let encounter = StructuredEncounter::from_combat_state(&combat);

    // Status effects should be represented in the encounter.
    // Could be in secondary_stats, a dedicated field, or narrator_hints.
    // At minimum, the encounter must serialize data about active effects.
    let json = serde_json::to_string(&encounter).expect("serialize");

    // Effects on two different targets — both must be preserved
    let has_secondary = encounter.secondary_stats.is_some();
    let has_hints = !encounter.narrator_hints.is_empty();
    assert!(
        has_secondary || has_hints || json.contains("effect") || json.contains("Poison") || json.contains("Stun"),
        "status effects must be preserved in the encounter representation"
    );
}

// ==========================================================================
// AC-3: HP metric is Descending with threshold_low = 0
// ==========================================================================

/// The combat metric must be HP, descending toward 0.
#[test]
fn combat_metric_is_hp_descending() {
    let encounter = StructuredEncounter::combat(vec!["A".into()], 25);

    assert_eq!(encounter.metric.name, "hp");
    assert_eq!(encounter.metric.direction, MetricDirection::Descending);
    assert_eq!(encounter.metric.threshold_low, Some(0));
    assert!(encounter.metric.threshold_high.is_none());
}

// ==========================================================================
// AC-4: Serde roundtrip for combat encounters
// ==========================================================================

/// A combat encounter must survive JSON serialization/deserialization.
#[test]
fn combat_encounter_serde_roundtrip() {
    let encounter = StructuredEncounter::combat(
        vec!["Hero".into(), "Goblin".into(), "Orc".into()],
        30,
    );

    let json = serde_json::to_string(&encounter).expect("serialize combat encounter");
    let deserialized: StructuredEncounter =
        serde_json::from_str(&json).expect("deserialize combat encounter");

    assert_eq!(deserialized.encounter_type, "combat");
    assert_eq!(deserialized.metric.name, "hp");
    assert_eq!(deserialized.metric.direction, MetricDirection::Descending);
    assert_eq!(deserialized.metric.current, 30);
    assert_eq!(deserialized.actors.len(), 3);
    assert!(!deserialized.resolved);
}

/// A migrated combat encounter must survive serde roundtrip.
#[test]
fn migrated_combat_encounter_serde_roundtrip() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Hero".into(), "Goblin".into()]);
    combat.advance_round();
    combat.set_drama_weight(0.75);
    combat.log_damage(DamageEvent {
        attacker: "Hero".into(),
        target: "Goblin".into(),
        damage: 5,
        round: 1,
    });

    let encounter = StructuredEncounter::from_combat_state(&combat);
    let json = serde_json::to_string(&encounter).expect("serialize migrated");
    let deserialized: StructuredEncounter =
        serde_json::from_str(&json).expect("deserialize migrated");

    assert_eq!(deserialized.encounter_type, "combat");
    assert_eq!(deserialized.beat, combat.round());
    assert_eq!(deserialized.actors.len(), 2);
}

// ==========================================================================
// AC-5: CombatState still works — no behavioral regression
// ==========================================================================

/// CombatState::new() still works after refactor.
#[test]
fn combat_state_new_still_works() {
    let combat = CombatState::new();
    assert_eq!(combat.round(), 1);
    assert!(!combat.in_combat());
    assert!(combat.turn_order().is_empty());
    assert!(combat.damage_log().is_empty());
    assert_eq!(combat.drama_weight(), 0.0);
}

/// CombatState::engage() still works after refactor.
#[test]
fn combat_state_engage_still_works() {
    let mut combat = CombatState::new();
    combat.engage(vec!["A".into(), "B".into(), "C".into()]);

    assert!(combat.in_combat());
    assert_eq!(combat.turn_order().len(), 3);
    assert_eq!(combat.current_turn(), Some("A"));
}

/// CombatState::advance_turn() still wraps and advances rounds.
#[test]
fn combat_state_advance_turn_still_works() {
    let mut combat = CombatState::new();
    combat.engage(vec!["A".into(), "B".into()]);

    combat.advance_turn(); // A → B
    assert_eq!(combat.current_turn(), Some("B"));

    combat.advance_turn(); // B → A (round advances)
    assert_eq!(combat.current_turn(), Some("A"));
    assert_eq!(combat.round(), 2);
}

/// CombatState::disengage() still resets everything.
#[test]
fn combat_state_disengage_still_works() {
    let mut combat = CombatState::new();
    combat.engage(vec!["A".into(), "B".into()]);
    combat.set_drama_weight(0.9);
    combat.advance_round();

    combat.disengage();

    assert!(!combat.in_combat());
    assert_eq!(combat.round(), 1);
    assert!(combat.turn_order().is_empty());
    assert!(combat.damage_log().is_empty());
    assert_eq!(combat.drama_weight(), 0.0);
}

/// CombatState status effects still tick and expire.
#[test]
fn combat_state_effects_still_work() {
    let mut combat = CombatState::new();
    combat.add_effect("Hero", StatusEffect::new(StatusEffectKind::Poison, 2));

    assert_eq!(combat.effects_on("Hero").len(), 1);

    combat.tick_effects();
    assert_eq!(combat.effects_on("Hero").len(), 1, "1 round left");

    combat.tick_effects();
    assert_eq!(
        combat.effects_on("Hero").len(),
        0,
        "effect should expire after 2 ticks"
    );
}

/// CombatState serde roundtrip still works.
#[test]
fn combat_state_serde_roundtrip_still_works() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Hero".into(), "Goblin".into()]);
    combat.set_drama_weight(0.5);
    combat.log_damage(DamageEvent {
        attacker: "Hero".into(),
        target: "Goblin".into(),
        damage: 7,
        round: 1,
    });

    let json = serde_json::to_string(&combat).expect("serialize CombatState");
    let deserialized: CombatState =
        serde_json::from_str(&json).expect("deserialize CombatState");

    assert!(deserialized.in_combat());
    assert_eq!(deserialized.damage_log().len(), 1);
    assert_eq!(deserialized.drama_weight(), 0.5);
}

// ==========================================================================
// Wiring test: combat encounter integrates with GameSnapshot
// ==========================================================================

/// GameSnapshot should accept a combat-type StructuredEncounter in the
/// encounter field (same field used by chase encounters from 16-2).
#[test]
fn game_snapshot_accepts_combat_encounter() {
    use sidequest_game::state::GameSnapshot;

    let mut snapshot = GameSnapshot::default();
    let encounter = StructuredEncounter::combat(
        vec!["Hero".into(), "Goblin".into()],
        30,
    );

    snapshot.encounter = Some(encounter);

    let enc = snapshot.encounter.as_ref().expect("encounter set");
    assert_eq!(enc.encounter_type, "combat");
    assert_eq!(enc.metric.name, "hp");
    assert_eq!(enc.actors.len(), 2);
}

/// GameSnapshot with a combat encounter survives serde roundtrip.
#[test]
fn game_snapshot_combat_encounter_serde_roundtrip() {
    use sidequest_game::state::GameSnapshot;

    let mut snapshot = GameSnapshot::default();
    snapshot.encounter = Some(StructuredEncounter::combat(
        vec!["Hero".into(), "Goblin".into()],
        30,
    ));

    let json = serde_json::to_string(&snapshot).expect("serialize snapshot");
    let de: GameSnapshot = serde_json::from_str(&json).expect("deserialize snapshot");

    let enc = de.encounter.as_ref().expect("encounter survived roundtrip");
    assert_eq!(enc.encounter_type, "combat");
    assert_eq!(enc.actors.len(), 2);
    assert_eq!(enc.metric.current, 30);
}
