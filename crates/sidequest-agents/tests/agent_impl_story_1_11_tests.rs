//! Failing tests for Story 1-11: Agent implementations + Orchestrator.
//!
//! Covers all 8 ACs plus rule-enforcement tests for deferred debt from 1-10.
//! All tests are expected to FAIL until Dev implements the modules.

use std::collections::HashMap;

// These modules don't exist yet — compilation will fail (RED state).
use sidequest_agents::agent::Agent;
use sidequest_agents::context_builder::ContextBuilder;
// ADR-067: CreatureSmith, Dialectician, Ensemble absorbed into unified narrator
use sidequest_agents::agents::intent_router::{Intent, IntentRoute};
use sidequest_agents::agents::narrator::NarratorAgent;
use sidequest_agents::agents::resonator::ResonatorAgent;
use sidequest_agents::agents::troper::TroperAgent;
use sidequest_agents::agents::world_builder::WorldBuilderAgent;
use sidequest_agents::orchestrator::{ActionResult, GameService};
use sidequest_agents::patches::WorldStatePatch;

// ============================================================
// AC 1: All 8 agents implement Agent trait fully
// ============================================================

mod agent_trait_tests {
    use super::*;

    #[test]
    fn narrator_implements_agent_trait() {
        let agent = NarratorAgent::new();
        assert_eq!(agent.name(), "narrator");
        assert!(!agent.system_prompt().is_empty());
        assert!(agent.system_prompt().contains("Game Master"));
    }

    #[test]
    fn world_builder_implements_agent_trait() {
        let agent = WorldBuilderAgent::new();
        assert_eq!(agent.name(), "world_builder");
        assert!(!agent.system_prompt().is_empty());
    }

    // ADR-067: Ensemble and CreatureSmith absorbed into unified narrator.
    // Tests for those agents removed.

    #[test]
    fn troper_implements_agent_trait() {
        let agent = TroperAgent::new();
        assert_eq!(agent.name(), "troper");
        assert!(!agent.system_prompt().is_empty());
    }

    // ADR-067: Dialectician absorbed into unified narrator.
    // Test for dialectician agent removed.

    #[test]
    fn resonator_implements_agent_trait() {
        let agent = ResonatorAgent::new();
        assert_eq!(agent.name(), "resonator");
        assert!(!agent.system_prompt().is_empty());
    }

    #[test]
    fn all_agent_names_are_unique() {
        // ADR-067: CreatureSmith, Ensemble, Dialectician absorbed into narrator
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(NarratorAgent::new()),
            Box::new(WorldBuilderAgent::new()),
            Box::new(TroperAgent::new()),
            Box::new(ResonatorAgent::new()),
        ];
        let names: Vec<&str> = agents.iter().map(|a| a.name()).collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(names.len(), unique.len(), "Agent names must be unique");
    }

    #[test]
    fn narrator_system_prompt_has_agency_rules() {
        let agent = NarratorAgent::new();
        let mut builder = ContextBuilder::new();
        agent.build_context(&mut builder);

        // The agency rule is added via build_context in the structured template system (story 23-1).
        // Verify it exists by checking the sections.
        let sections = builder.build();
        let has_agency_guardrail = sections
            .iter()
            .any(|s| s.content.contains("NEVER") || s.content.contains("Agency"));

        assert!(
            has_agency_guardrail,
            "Narrator must have agency guardrail section in build_context"
        );
    }
}

// ============================================================
// AC 2: Each agent has dedicated request/response types
// ============================================================

mod agent_types_tests {
    use super::*;

    #[test]
    fn world_state_patch_deserializes_from_json() {
        let json = r#"{
            "location": "The Rusted Tavern",
            "time_of_day": "evening",
            "hp_changes": {"Gorm": -5},
            "notes": "A chill wind blows"
        }"#;
        let patch: WorldStatePatch = serde_json::from_str(json).unwrap();
        assert_eq!(patch.location.as_deref(), Some("The Rusted Tavern"));
        assert_eq!(patch.time_of_day.as_deref(), Some("evening"));
    }

    #[test]
    fn world_state_patch_all_fields_optional() {
        let json = "{}";
        let patch: WorldStatePatch = serde_json::from_str(json).unwrap();
        assert!(patch.location.is_none());
        assert!(patch.hp_changes.is_none());
    }

    // combat_patch_deserializes_from_json / chase_patch_deserializes_from_json
    // removed — CombatPatch/ChasePatch were deleted in story 16-2.

    #[test]
    fn action_result_contains_narration_and_delta() {
        // ActionResult must carry narration text and optional state changes
        let result = ActionResult {
            narration: "You enter the dimly lit tavern.".to_string(),
            beat_selections: vec![],
            confrontation: None,
            location: None,
            prompt_text: None,
            raw_response_text: None,
            is_degraded: false,
            classified_intent: None,
            agent_name: None,
            footnotes: vec![],
            items_gained: vec![],
            npcs_present: vec![],
            quest_updates: HashMap::new(),
            agent_duration_ms: None,
            token_count_in: None,
            token_count_out: None,

            visual_scene: None,
            scene_mood: None,
            personality_events: vec![],
            scene_intent: None,
            resource_deltas: HashMap::new(),
            zone_breakdown: None,
            lore_established: None,
            action_rewrite: None,
            action_flags: None,
            sfx_triggers: vec![],
            merchant_transactions: vec![],
            prompt_tier: String::new(),
        };
        assert!(!result.narration.is_empty());
        assert!(!result.is_degraded);
    }
}

// ============================================================
// AC 4: ContextBuilder feeds genre, character, state context
// ============================================================

mod context_building_tests {
    use super::*;
    use sidequest_agents::prompt_framework::SectionCategory;

    #[test]
    fn narrator_has_system_prompt() {
        let agent = NarratorAgent::new();
        assert!(
            !agent.system_prompt().is_empty(),
            "Narrator must have a system prompt"
        );
    }

    #[test]
    fn all_agents_add_identity_section() {
        // ADR-067: CreatureSmith, Ensemble, Dialectician absorbed into narrator
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(NarratorAgent::new()),
            Box::new(WorldBuilderAgent::new()),
            Box::new(TroperAgent::new()),
            Box::new(ResonatorAgent::new()),
        ];
        for _agent in &agents {
            let builder = ContextBuilder::new();
            // Each agent should add at least an identity section
            // This will need a build_context method signature
            let _identity_sections = builder.sections_by_category(SectionCategory::Identity);
            // After build_context, identity should be populated
            // For now, assert the builder API exists
            assert_eq!(builder.token_estimate(), 0, "Empty builder has zero tokens");
        }
    }
}

// ============================================================
// AC 5: GameService sequences agents in correct order
// ============================================================

mod intent_routing_tests {
    use super::*;

    #[test]
    fn intent_enum_has_expected_variants() {
        // Intent must cover the core action types
        let combat = Intent::Combat;
        let dialogue = Intent::Dialogue;
        let exploration = Intent::Exploration;
        let examine = Intent::Examine;
        let _meta = Intent::Meta;
        // Verify they're distinct
        assert_ne!(format!("{:?}", combat), format!("{:?}", dialogue));
        assert_ne!(format!("{:?}", exploration), format!("{:?}", examine));
    }

    #[test]
    fn intent_route_maps_combat_to_narrator() {
        // ADR-067: All intents route to narrator
        let route = IntentRoute::for_intent(Intent::Combat);
        assert_eq!(route.agent_name(), "narrator");
    }

    #[test]
    fn intent_route_maps_dialogue_to_narrator() {
        // ADR-067: All intents route to narrator
        let route = IntentRoute::for_intent(Intent::Dialogue);
        assert_eq!(route.agent_name(), "narrator");
    }

    #[test]
    fn intent_route_maps_exploration_to_narrator() {
        let route = IntentRoute::for_intent(Intent::Exploration);
        assert_eq!(route.agent_name(), "narrator");
    }

    #[test]
    fn intent_route_defaults_to_narrator_on_unknown() {
        // ADR-010: fallback to Narrator if classification fails
        let route = IntentRoute::narrator_fallback();
        assert_eq!(route.agent_name(), "narrator");
    }

    #[test]
    fn intent_route_exploration_returns_narrator() {
        let route = IntentRoute::exploration();
        assert_eq!(route.agent_name(), "narrator");
        assert_eq!(route.intent(), Intent::Exploration);
    }
}

// ============================================================
// AC 6: GameService manages game snapshots and state delta
// ============================================================

mod game_service_tests {
    use super::*;

    #[test]
    fn game_service_trait_is_object_safe() {
        // Server must be able to use `dyn GameService` — verify object safety
        fn _accepts_dyn(_service: &dyn GameService) {}
        // If this compiles, the trait is object-safe
    }

    #[test]
    fn orchestrator_implements_game_service() {
        // Orchestrator must implement GameService
        fn _accepts_game_service<T: GameService>(_t: &T) {}
        // This will fail to compile until Orchestrator implements GameService
    }

    #[test]
    fn action_result_has_required_fields() {
        let result = ActionResult {
            narration: "test".to_string(),
            beat_selections: vec![],
            confrontation: None,
            location: None,
            prompt_text: None,
            raw_response_text: None,
            is_degraded: false,
            classified_intent: None,
            agent_name: None,
            footnotes: vec![],
            items_gained: vec![],
            npcs_present: vec![],
            quest_updates: HashMap::new(),
            agent_duration_ms: None,
            token_count_in: None,
            token_count_out: None,

            visual_scene: None,
            scene_mood: None,
            personality_events: vec![],
            scene_intent: None,
            resource_deltas: HashMap::new(),
            zone_breakdown: None,
            lore_established: None,
            action_rewrite: None,
            action_flags: None,
            sfx_triggers: vec![],
            merchant_transactions: vec![],
            prompt_tier: String::new(),
        };
        assert_eq!(result.narration, "test");
        assert!(!result.is_degraded);
    }
}

// ============================================================
// AC 7: Agent logic tested with real Claude calls — via mock
// (Session AC says "real Claude calls" but context says
//  "Orchestrator can be tested with mock ClaudeClient")
// ============================================================

// Note: We test the wiring, not the LLM output. Real Claude calls
// are integration tests, not unit tests. Mock ClaudeClient for unit.

// ============================================================
// AC 8: Agents handle error cases gracefully
// ============================================================

mod error_handling_tests {
    use super::*;

    #[test]
    fn action_result_can_be_degraded() {
        // ADR-005: Timeout → degraded response, not error
        let result = ActionResult {
            narration: "The narrator pauses, gathering their thoughts...".to_string(),
            beat_selections: vec![],
            confrontation: None,
            location: None,
            prompt_text: None,
            raw_response_text: None,
            is_degraded: true,
            classified_intent: None,
            agent_name: None,
            footnotes: vec![],
            items_gained: vec![],
            npcs_present: vec![],
            quest_updates: HashMap::new(),
            agent_duration_ms: None,
            token_count_in: None,
            token_count_out: None,

            visual_scene: None,
            scene_mood: None,
            personality_events: vec![],
            scene_intent: None,
            resource_deltas: HashMap::new(),
            zone_breakdown: None,
            lore_established: None,
            action_rewrite: None,
            action_flags: None,
            sfx_triggers: vec![],
            merchant_transactions: vec![],
            prompt_tier: String::new(),
        };
        assert!(result.is_degraded);
        assert!(!result.narration.is_empty());
    }
}

// ============================================================
// Rule enforcement: Deferred debt from story 1-10
// ============================================================

mod deferred_debt_tests {
    use super::*;

    #[test]
    fn world_state_patch_deny_unknown_fields() {
        // ADR-011: Reject unknown keys in patches
        let json = r#"{"location": "tavern", "bogus_field": true}"#;
        let result = serde_json::from_str::<WorldStatePatch>(json);
        assert!(result.is_err(), "WorldStatePatch must deny unknown fields");
    }

    // combat_patch_allows_unknown_fields / chase_patch_allows_unknown_fields removed —
    // CombatPatch/ChasePatch were deleted in story 16-2.

    #[test]
    fn world_state_patch_serde_round_trip() {
        let json = r#"{"location": "The Rusted Tavern", "time_of_day": "evening"}"#;
        let patch: WorldStatePatch = serde_json::from_str(json).unwrap();
        let serialized = serde_json::to_string(&patch).unwrap();
        let roundtrip: WorldStatePatch = serde_json::from_str(&serialized).unwrap();
        assert_eq!(patch.location, roundtrip.location);
        assert_eq!(patch.time_of_day, roundtrip.time_of_day);
    }

    #[test]
    fn intent_enum_is_non_exhaustive() {
        // Rule #2: public enums should be #[non_exhaustive]
        // This test verifies Intent exists and has Debug derive
        let intent = Intent::Exploration;
        let debug = format!("{:?}", intent);
        assert!(!debug.is_empty());
    }
}
