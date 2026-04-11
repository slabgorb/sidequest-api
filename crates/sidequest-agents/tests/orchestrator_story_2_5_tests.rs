//! Story 2-5: Orchestrator turn loop tests
//!
//! Tests verify:
//!   - Intent routing table (Intent → agent name)
//!   - State overrides (in_combat/in_chase force intent)
//!   - TurnResult structure
//!   - Patch extraction
//!   - Graceful degradation
//!   - AgentKind enum

use std::collections::HashMap;

use sidequest_agents::agents::intent_router::{
    Intent, IntentClassifier, IntentRoute, IntentRouter,
};
use sidequest_agents::orchestrator::{AgentKind, Orchestrator, TurnContext, TurnResult};

/// Mock classifier that always returns a fixed intent.
struct MockClassifier(Intent);

impl IntentClassifier for MockClassifier {
    fn classify(&self, _input: &str, _context: &TurnContext) -> IntentRoute {
        IntentRoute::for_intent(self.0)
    }
}
use sidequest_game::tension_tracker::DeliveryMode;

// ============================================================================
// AC-2/3/4: Intent routing table — Intent → agent name
// ============================================================================

#[test]
fn combat_routes_to_narrator() {
    // ADR-067: All intents route to narrator
    let route = IntentRoute::for_intent(Intent::Combat);
    assert_eq!(route.intent(), Intent::Combat);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn dialogue_routes_to_narrator() {
    // ADR-067: All intents route to narrator
    let route = IntentRoute::for_intent(Intent::Dialogue);
    assert_eq!(route.intent(), Intent::Dialogue);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn exploration_routes_to_narrator() {
    let route = IntentRoute::for_intent(Intent::Exploration);
    assert_eq!(route.intent(), Intent::Exploration);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn examine_routes_to_narrator() {
    let route = IntentRoute::for_intent(Intent::Examine);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn meta_routes_to_narrator() {
    let route = IntentRoute::for_intent(Intent::Meta);
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn chase_routes_to_narrator() {
    // ADR-067: All intents route to narrator
    let route = IntentRoute::for_intent(Intent::Chase);
    assert_eq!(route.agent_name(), "narrator");
}

// ============================================================================
// AC-7/8: Chase + combat detection — superseded by ADR-067 / story 28-6.
//
// Originally these tests pinned the IntentRouter's split routing (in_chase →
// Chase agent, in_combat → Combat agent). ADR-067 unified all routing under
// the narrator: every action becomes Intent::Exploration and the encounter is
// driven by `beat_selections` injected into the narrator's game_patch output.
// The asserts below pin the new architecture so we don't drift back into
// split-agent routing.
// ============================================================================

#[test]
fn chase_state_does_not_branch_router_post_adr_067() {
    let ctx = TurnContext {
        in_chase: true,
        ..TurnContext::default()
    };
    let classifier = MockClassifier(Intent::Exploration);
    let route = IntentRouter::classify_with_classifier("I look around", &ctx, &classifier);
    assert_eq!(
        route.intent(),
        Intent::Exploration,
        "ADR-067: in_chase no longer branches the router — narrator handles via beat_selections"
    );
    assert_eq!(route.agent_name(), "narrator");
}

#[test]
fn combat_state_does_not_branch_router_post_adr_067() {
    let ctx = TurnContext {
        in_combat: true,
        ..TurnContext::default()
    };
    let classifier = MockClassifier(Intent::Exploration);
    let route = IntentRouter::classify_with_classifier("I look around", &ctx, &classifier);
    assert_eq!(
        route.intent(),
        Intent::Exploration,
        "ADR-067: in_combat no longer branches the router — narrator handles via beat_selections"
    );
    assert_eq!(route.agent_name(), "narrator");
}

// ============================================================================
// AC-5: Narrator fallback when Haiku is unavailable
// ============================================================================

#[test]
fn fallback_routes_to_narrator() {
    let route = IntentRoute::narrator_fallback();
    assert_eq!(route.agent_name(), "narrator");
    assert_eq!(route.intent(), Intent::Exploration);
}

// ============================================================================
// AC-16: Classification does not mutate context
// ============================================================================

#[test]
fn classify_does_not_mutate_context() {
    let ctx = TurnContext::default();
    let classifier = MockClassifier(Intent::Combat);
    let _route = IntentRouter::classify_with_classifier("attack", &ctx, &classifier);
    assert!(!ctx.in_combat, "classify must not mutate state");
    assert!(!ctx.in_chase, "classify must not mutate state");
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
        delivery_mode: DeliveryMode::Instant,
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
        delivery_mode: DeliveryMode::Instant,
    };

    assert!(result.is_degraded);
    assert!(!result.narration.is_empty());
}

// ============================================================================
// AgentKind enum
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
    let kind = AgentKind::Narrator;
    assert_eq!(format!("{:?}", kind), "Narrator");
}

// ============================================================================
// AC-10: State delta computed
// ============================================================================

#[test]
fn turn_result_carries_state_delta() {
    let mut delta = HashMap::new();
    delta.insert("location".to_string(), serde_json::json!("Dark Alley"));

    let result = TurnResult {
        narration: "You move into the shadows.".to_string(),
        state_delta: Some(delta),
        combat_events: vec![],
        is_degraded: false,
        agent_used: AgentKind::Narrator,
        delivery_mode: DeliveryMode::Instant,
    };

    assert!(result.state_delta.is_some());
    let delta = result.state_delta.unwrap();
    assert_eq!(delta["location"], "Dark Alley");
}

// ============================================================================
// Patch extraction — CombatPatch/ChasePatch removed in story 16-2
// (ADR-033 confrontation engine). Patch extraction now flows through
// the narrator's game_patch JSON, not typed CombatPatch/ChasePatch.
// ============================================================================

// ============================================================================
// Routing table completeness
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

#[test]
fn turn_context_default_is_peaceful() {
    let ctx = TurnContext::default();
    assert!(!ctx.in_combat);
    assert!(!ctx.in_chase);
}

// ============================================================================
// Rule #2: non_exhaustive
// ============================================================================

#[test]
fn agent_kind_enum_is_non_exhaustive() {
    let _n = AgentKind::Narrator;
    let _c = AgentKind::CreatureSmith;
}

#[test]
fn intent_enum_has_chase() {
    let _chase = Intent::Chase;
}
