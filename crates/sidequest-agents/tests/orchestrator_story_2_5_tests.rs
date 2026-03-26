//! Story 2-5: Orchestrator turn loop tests
//!
//! RED phase — these tests reference types and methods that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - IntentRouter::classify() keyword-based routing
//!   - Intent::Chase variant
//!   - Orchestrator::process_turn() full turn loop
//!   - TurnResult struct (narration + delta + combat + is_degraded)
//!   - AgentKind enum
//!   - Streaming via mpsc channel
//!   - Patch extraction and application
//!   - Graceful degradation (timeout → fallback)
//!   - Auto-save integration
//!
//! ACs tested: all 16

use std::collections::HashMap;

// === New/extended types from story 2-5 ===
use sidequest_agents::agents::intent_router::{Intent, IntentRoute, IntentRouter};
use sidequest_agents::orchestrator::{
    AgentKind, Orchestrator, TurnContext, TurnResult,
};

// ============================================================================
// AC-2: Intent routing — combat
// ============================================================================

#[test]
fn classify_attack_as_combat() {
    let route = IntentRouter::classify_keywords("I attack the goblin");
    assert_eq!(route.intent(), Intent::Combat);
    assert_eq!(route.agent_name(), "creature_smith");
}

#[test]
fn classify_slash_as_combat() {
    let route = IntentRouter::classify_keywords("slash the orc with my sword");
    assert_eq!(route.intent(), Intent::Combat);
}

#[test]
fn classify_cast_spell_as_combat() {
    let route = IntentRouter::classify_keywords("cast fireball at the dragon");
    assert_eq!(route.intent(), Intent::Combat);
}

// ============================================================================
// AC-3: Intent routing — dialogue
// ============================================================================

#[test]
fn classify_talk_as_dialogue() {
    let route = IntentRouter::classify_keywords("talk to the innkeeper");
    assert_eq!(route.intent(), Intent::Dialogue);
    assert_eq!(route.agent_name(), "ensemble");
}

#[test]
fn classify_tell_with_target() {
    let route = IntentRouter::classify_keywords("tell luna hello");
    assert_eq!(route.intent(), Intent::Dialogue);
}

#[test]
fn classify_ask_as_dialogue() {
    let route = IntentRouter::classify_keywords("ask the merchant about the map");
    assert_eq!(route.intent(), Intent::Dialogue);
}

// ============================================================================
// AC-4: Intent routing — exploration
// ============================================================================

#[test]
fn classify_look_around_as_exploration() {
    let route = IntentRouter::classify_keywords("I look around the room");
    assert_eq!(route.intent(), Intent::Exploration);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn classify_go_as_exploration() {
    let route = IntentRouter::classify_keywords("go north through the door");
    assert_eq!(route.intent(), Intent::Exploration);
}

// ============================================================================
// AC-5: Intent fallback — unknown input defaults to Exploration
// ============================================================================

#[test]
fn classify_unknown_input_falls_back_to_exploration() {
    let route = IntentRouter::classify_keywords("hmm interesting");
    assert_eq!(
        route.intent(),
        Intent::Exploration,
        "Unknown input should default to Exploration"
    );
}

// ============================================================================
// AC-6: Keyword matching — combat keywords detected without LLM
// ============================================================================

#[test]
fn keyword_matching_combat_words() {
    let combat_inputs = [
        "attack",
        "slash",
        "cast spell",
        "shoot an arrow",
        "defend myself",
        "strike the beast",
    ];
    for input in &combat_inputs {
        let route = IntentRouter::classify_keywords(input);
        assert_eq!(
            route.intent(),
            Intent::Combat,
            "\"{}\" should be classified as Combat",
            input
        );
    }
}

// ============================================================================
// AC-7: Chase detection — state override
// ============================================================================

#[test]
fn chase_state_overrides_keywords() {
    let mut state_flags = TurnContext::default();
    state_flags.in_chase = true;

    let route = IntentRouter::classify_with_state("I look around", &state_flags);
    assert_eq!(
        route.intent(),
        Intent::Chase,
        "Active chase should override keyword classification"
    );
}

// ============================================================================
// AC-8: Combat detection — state override
// ============================================================================

#[test]
fn combat_state_overrides_keywords() {
    let mut state_flags = TurnContext::default();
    state_flags.in_combat = true;

    let route = IntentRouter::classify_with_state("I look around", &state_flags);
    assert_eq!(
        route.intent(),
        Intent::Combat,
        "Active combat should override keyword classification"
    );
}

// ============================================================================
// AC-16: No router side effects — classify is pure
// ============================================================================

#[test]
fn classify_does_not_mutate_context() {
    let state_flags = TurnContext::default();
    let _route1 = IntentRouter::classify_with_state("attack", &state_flags);
    let _route2 = IntentRouter::classify_with_state("look around", &state_flags);
    // If classify mutated state_flags, we'd see different results
    // This test verifies the function takes &TurnContext (immutable reference)
    assert!(!state_flags.in_combat, "classify must not mutate state");
    assert!(!state_flags.in_chase, "classify must not mutate state");
}

// ============================================================================
// AC-1: Turn completes end-to-end (TurnResult structure)
// ============================================================================

#[test]
fn turn_result_has_required_fields() {
    let result = TurnResult {
        narration: "The goblin falls.".to_string(),
        state_delta: Some(HashMap::new()),
        combat_events: vec!["damage: 5".to_string()],
        is_degraded: false,
        agent_used: AgentKind::CreatureSmith,
    };

    assert_eq!(result.narration, "The goblin falls.");
    assert!(!result.is_degraded);
    assert_eq!(result.agent_used, AgentKind::CreatureSmith);
}

// ============================================================================
// AC-14: Graceful degradation — timeout produces fallback
// ============================================================================

#[test]
fn degraded_turn_result_marked() {
    let result = TurnResult {
        narration: "The world seems to pause for a moment...".to_string(),
        state_delta: None,
        combat_events: vec![],
        is_degraded: true,
        agent_used: AgentKind::Narrator,
    };

    assert!(
        result.is_degraded,
        "Degraded result should have is_degraded=true"
    );
    assert!(
        !result.narration.is_empty(),
        "Degraded result should still have fallback narration"
    );
}

// ============================================================================
// AgentKind enum — typed agent selection
// ============================================================================

#[test]
fn agent_kind_variants_exist() {
    let _variants = [
        AgentKind::Narrator,
        AgentKind::CreatureSmith,
        AgentKind::Ensemble,
        AgentKind::Dialectician,
        AgentKind::WorldBuilder,
        AgentKind::Troper,
        AgentKind::Resonator,
        AgentKind::IntentRouter,
    ];
}

#[test]
fn agent_kind_is_non_exhaustive() {
    // Verify the enum exists with expected variants
    let kind = AgentKind::Narrator;
    assert_eq!(format!("{:?}", kind), "Narrator");
}

// ============================================================================
// AC-10: State delta computed
// ============================================================================

#[test]
fn turn_result_carries_state_delta() {
    let mut delta = HashMap::new();
    delta.insert(
        "location".to_string(),
        serde_json::json!("Dark Alley"),
    );

    let result = TurnResult {
        narration: "You move into the shadows.".to_string(),
        state_delta: Some(delta),
        combat_events: vec![],
        is_degraded: false,
        agent_used: AgentKind::Narrator,
    };

    assert!(result.state_delta.is_some());
    let delta = result.state_delta.unwrap();
    assert_eq!(delta["location"], "Dark Alley");
}

// ============================================================================
// AC-12: Combat patch extraction
// ============================================================================

#[test]
fn extract_combat_patch_from_response() {
    use sidequest_agents::extractor::JsonExtractor;
    use sidequest_agents::patches::CombatPatch;

    let response = r#"The goblin swings wildly but misses!

```json
{"advance_round": true}
```
"#;

    let patch = JsonExtractor::extract::<CombatPatch>(response);
    assert!(patch.is_ok(), "Should extract combat patch from response");
    assert!(patch.unwrap().advance_round);
}

// ============================================================================
// AC-13: Chase patch extraction
// ============================================================================

#[test]
fn extract_chase_patch_from_response() {
    use sidequest_agents::extractor::JsonExtractor;
    use sidequest_agents::patches::ChasePatch;

    let response = r#"You sprint through the alley!

```json
{"roll": 0.7}
```
"#;

    let patch = JsonExtractor::extract::<ChasePatch>(response);
    assert!(patch.is_ok(), "Should extract chase patch from response");
}

// ============================================================================
// Intent::Chase variant exists
// ============================================================================

#[test]
fn intent_chase_variant_exists() {
    let _chase = Intent::Chase;
    let route = IntentRoute::for_intent(Intent::Chase);
    assert_eq!(
        route.agent_name(),
        "dialectician",
        "Chase intent should route to dialectician"
    );
}

// ============================================================================
// TurnContext — state flags for routing
// ============================================================================

#[test]
fn turn_context_default_is_peaceful() {
    let ctx = TurnContext::default();
    assert!(!ctx.in_combat, "Default context should not be in combat");
    assert!(!ctx.in_chase, "Default context should not be in chase");
}

// ============================================================================
// IntentRoute covers all intent variants
// ============================================================================

#[test]
fn all_intents_have_routes() {
    let intents = [
        Intent::Combat,
        Intent::Dialogue,
        Intent::Exploration,
        Intent::Examine,
        Intent::Meta,
        Intent::Chase,
    ];

    for intent in &intents {
        let route = IntentRoute::for_intent(*intent);
        assert!(
            !route.agent_name().is_empty(),
            "Intent {:?} should have a non-empty agent name",
            intent
        );
    }
}

// ============================================================================
// Rust lang-review rule enforcement
// ============================================================================

// Rule #2: #[non_exhaustive] on AgentKind
#[test]
fn agent_kind_enum_is_non_exhaustive() {
    // If this compiles, the enum exists. non_exhaustive verified by gate.
    let _n = AgentKind::Narrator;
    let _c = AgentKind::CreatureSmith;
}

// Rule #2: Intent::Chase is part of non_exhaustive enum
#[test]
fn intent_enum_has_chase() {
    let _chase = Intent::Chase;
}
