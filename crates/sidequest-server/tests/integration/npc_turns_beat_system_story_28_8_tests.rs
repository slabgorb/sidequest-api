//! Story 28-8: NPC turns through beat system — NPCs mechanically act every round
//!
//! The dispatch loop currently processes only the player's beat (from scene_intent).
//! This story wires NPC beat selections from ActionResult.beat_selections into the
//! dispatch loop, so every NPC actor gets apply_beat() called with mechanical
//! resolution and OTEL instrumentation.
//!
//! ACs tested:
//!   AC-NPC-Beat-Loop:           Dispatch iterates beat_selections for non-player actors
//!   AC-Combat-NPC-Default:      Combat NPCs default to "attack" targeting a player
//!   AC-NonCombat-NPC-Selection: Non-combat NPCs select beats based on disposition/role
//!   AC-Apply-Beat-Per-Actor:    apply_beat() called for each NPC beat selection
//!   AC-OTEL-NPC-Beat:           encounter.npc_beat event with npc_name, beat_id, target, stat_check_result
//!   AC-Wiring-BeatSelections:   beat_selections field from ActionResult has non-test consumer in dispatch

// =========================================================================
// Source inspection tests — verify code patterns exist in production
// =========================================================================

/// AC-Wiring-BeatSelections: beat_selections from ActionResult must be consumed
/// in the dispatch pipeline, not just defined in the struct.
#[test]
fn dispatch_consumes_beat_selections_from_action_result() {
    let dispatch_src = include_str!("../../src/dispatch/mod.rs");

    // beat_selections must be read from the ActionResult in dispatch
    assert!(
        dispatch_src.contains("beat_selections"),
        "dispatch/mod.rs must reference beat_selections — \
         currently only scene_intent (single beat) is consumed. \
         The beat_selections Vec<BeatSelection> from ActionResult \
         must be iterated to dispatch NPC turns."
    );

    // Must not just mention it in a comment — needs actual field access
    let has_field_access =
        dispatch_src.contains(".beat_selections") || dispatch_src.contains("beat_selections.");
    assert!(
        has_field_access,
        "dispatch/mod.rs must access .beat_selections as a field, \
         not just mention it in a comment"
    );
}

/// AC-NPC-Beat-Loop: The dispatch loop must iterate over NPC beat selections,
/// not just process the player's single beat.
#[test]
fn dispatch_loops_over_npc_beat_selections() {
    let dispatch_src = include_str!("../../src/dispatch/mod.rs");

    // There must be a loop over beat_selections (for, iter, etc.)
    let has_iteration = dispatch_src.contains("for ") && dispatch_src.contains("beat_selection")
        || dispatch_src.contains("beat_selections.iter()")
        || dispatch_src.contains("beat_selections.into_iter()");

    assert!(
        has_iteration,
        "dispatch/mod.rs must iterate over beat_selections to process \
         each NPC's beat. Currently only a single beat_id from scene_intent \
         is dispatched."
    );
}

/// AC-NPC-Beat-Loop: NPC actors must be distinguished from the player.
/// The loop should skip the player's beat (already handled via scene_intent)
/// or handle all actors uniformly.
#[test]
fn dispatch_filters_or_handles_npc_actors() {
    let dispatch_src = include_str!("../../src/dispatch/mod.rs");

    // Must distinguish NPC beats from player beats
    let handles_actor_filtering = dispatch_src.contains("actor")
        && (dispatch_src.contains("Player")
            || dispatch_src.contains("player")
            || dispatch_src.contains("is_npc")
            || dispatch_src.contains("npc_beat"));

    assert!(
        handles_actor_filtering,
        "dispatch/mod.rs must filter or route beats by actor type \
         (NPC vs Player) when processing beat_selections"
    );
}

/// AC-Apply-Beat-Per-Actor: apply_beat must be called for each NPC's beat selection,
/// not just once for the player.
#[test]
fn apply_beat_called_per_npc_actor() {
    let dispatch_src = include_str!("../../src/dispatch/mod.rs");

    // Post-37-14: the canonical call site is `beat::apply_beat_dispatch(`.
    // The `beat::` module prefix and the opening paren force a real qualified
    // call expression — a comment mentioning `apply_beat()` elsewhere in the
    // file cannot satisfy this pattern. Reviewer pass-2 finding #7: the
    // previous OR chain contained a bare `contains("apply_beat")` substring
    // that was tautologically true because dispatch/mod.rs has `apply_beat()`
    // fragments in multiple comments (every comment would satisfy it and the
    // regression guard became a rubber stamp).
    let calls_beat_dispatch_in_loop = dispatch_src.contains("beat::apply_beat_dispatch(");

    assert!(
        calls_beat_dispatch_in_loop,
        "dispatch/mod.rs must call beat dispatch/apply for each NPC actor"
    );

    // The function must accept or use an actor/npc_name parameter
    let has_actor_param = dispatch_src.contains("npc_name")
        || dispatch_src.contains("actor_name")
        || dispatch_src.contains("selection.actor");

    assert!(
        has_actor_param,
        "Beat dispatch must track which actor (NPC) is performing the beat — \
         needed for OTEL attribution and targeting"
    );
}

// =========================================================================
// OTEL instrumentation tests
// =========================================================================

/// AC-OTEL-NPC-Beat: Every NPC action must emit an encounter.npc_beat OTEL event.
#[test]
fn otel_npc_beat_event_emitted() {
    let dispatch_src = include_str!("../../src/dispatch/mod.rs");

    assert!(
        dispatch_src.contains("encounter.npc_beat") || dispatch_src.contains("npc_beat"),
        "dispatch/mod.rs must emit an 'encounter.npc_beat' OTEL event \
         for each NPC beat dispatched. This is how the GM panel verifies \
         NPCs are mechanically acting, not just narratively improvising."
    );
}

/// AC-OTEL-NPC-Beat: The OTEL event must include npc_name, beat_id, target, stat_check_result.
#[test]
fn otel_npc_beat_event_has_required_fields() {
    let dispatch_src = include_str!("../../src/dispatch/mod.rs");

    // Each required field must appear in the OTEL event builder
    for field in &["npc_name", "beat_id", "target", "stat_check"] {
        assert!(
            dispatch_src.contains(field),
            "OTEL npc_beat event must include field '{}' — \
             the GM panel needs this to show exactly what each NPC did",
            field
        );
    }
}

// =========================================================================
// EncounterActor tests — verify actors are properly tracked
// =========================================================================

/// AC-NPC-Beat-Loop: EncounterActor must be usable to identify NPC participants.
#[test]
fn encounter_actor_identifies_npcs_in_encounter() {
    use sidequest_game::encounter::{EncounterActor, StructuredEncounter};

    // Create a combat encounter with NPCs
    let encounter = StructuredEncounter::combat(
        vec![
            "Player".to_string(),
            "Goblin".to_string(),
            "Orc".to_string(),
        ],
        20,
    );

    // Must have 3 actors
    assert_eq!(
        encounter.actors.len(),
        3,
        "Combat encounter must track all participants"
    );

    // Can identify NPC actors (non-player)
    let npc_actors: Vec<&EncounterActor> = encounter
        .actors
        .iter()
        .filter(|a| a.name != "Player")
        .collect();
    assert_eq!(npc_actors.len(), 2, "Should have 2 NPC actors");
    assert_eq!(npc_actors[0].name, "Goblin");
    assert_eq!(npc_actors[1].name, "Orc");
}

// =========================================================================
// BeatSelection struct tests — verify NPC beat selection format
// =========================================================================

/// AC-Combat-NPC-Default: Combat NPC beat selections must default to "attack".
#[test]
fn combat_npc_default_beat_is_attack() {
    use sidequest_agents::orchestrator::BeatSelection;

    // A combat NPC beat selection defaults to "attack"
    let npc_beat = BeatSelection {
        actor: "Goblin".to_string(),
        beat_id: "attack".to_string(),
        target: Some("Player".to_string()),
    };

    assert_eq!(npc_beat.actor, "Goblin");
    assert_eq!(npc_beat.beat_id, "attack");
    assert_eq!(npc_beat.target, Some("Player".to_string()));
}

/// AC-NonCombat-NPC-Selection: Non-combat NPCs can select beats other than "attack".
#[test]
fn non_combat_npc_can_select_non_attack_beats() {
    use sidequest_agents::orchestrator::BeatSelection;

    // A negotiation NPC might bluff or concede
    let npc_beat = BeatSelection {
        actor: "Merchant".to_string(),
        beat_id: "bluff".to_string(),
        target: None,
    };

    assert_eq!(npc_beat.beat_id, "bluff");
    assert!(
        npc_beat.target.is_none(),
        "Non-targeted beats should have no target"
    );
}

// =========================================================================
// Integration: NPC beat dispatch function existence
// =========================================================================

/// AC-NPC-Beat-Loop: There must be a function or code path that dispatches
/// NPC beats specifically (not just reusing the player beat path).
#[test]
fn npc_beat_dispatch_function_exists() {
    let dispatch_src = include_str!("../../src/dispatch/mod.rs");

    // Must have a dedicated NPC beat dispatch path or handle NPC actors in the existing one
    let has_npc_dispatch = dispatch_src.contains("dispatch_npc_beat")
        || dispatch_src.contains("npc_beat_selection")
        || (dispatch_src.contains("beat_selections") && dispatch_src.contains("actor"));

    assert!(
        has_npc_dispatch,
        "dispatch/mod.rs must have a code path for dispatching NPC beat selections — \
         either a dedicated function or NPC handling within the existing beat dispatch"
    );
}

/// AC-Wiring-BeatSelections: The dispatch pipeline must process multiple beats
/// per turn (one per actor), not just a single beat.
#[test]
fn dispatch_handles_multiple_beats_per_turn() {
    let dispatch_src = include_str!("../../src/dispatch/mod.rs");

    // The old pattern: single beat from scene_intent
    // The new pattern: iterate beat_selections for all actors
    // Both should exist (backward compat) but beat_selections must be processed
    let processes_multiple = dispatch_src.contains("beat_selections")
        && (dispatch_src.contains("for ") || dispatch_src.contains(".iter()"));

    assert!(
        processes_multiple,
        "dispatch/mod.rs must process multiple beat_selections per turn — \
         one for each actor (player + NPCs). Currently only a single \
         scene_intent beat is dispatched."
    );
}
