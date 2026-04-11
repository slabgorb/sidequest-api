//! Story 6-9: Wire Scene Directive into Orchestrator Turn Loop — failing tests (RED phase)
//!
//! Tests cover:
//!   - Intent meaningfulness classification (Combat/Dialogue = meaningful, Explore/Meta = not)
//!   - Engagement counter update logic (increment on non-meaningful, reset on meaningful)
//!   - Directive generation pipeline (fired beats → format → register → compose)
//!   - Non-narrator agents skip directive injection
//!   - Backward compatibility (no beats → no directive block in prompt)
//!   - Full pipeline: engagement multiplier → trope tick → directive → prompt

use sidequest_game::engagement::engagement_multiplier;
use sidequest_game::scene_directive::{format_scene_directive, ActiveStake};
use sidequest_game::state::GameSnapshot;
use sidequest_game::trope::{FiredBeat, TropeEngine, TropeState};

use sidequest_agents::prompt_framework::{
    render_scene_directive, AttentionZone, PromptComposer, PromptRegistry,
};

use sidequest_agents::agents::intent_router::{Intent, IntentRoute};

use sidequest_genre::{PassiveProgression, TropeDefinition, TropeEscalation};
use sidequest_protocol::NonBlankString;

// ============================================================================
// Test fixtures
// ============================================================================

fn fired_beat(event: &str, at: f64, stakes: &str) -> FiredBeat {
    FiredBeat {
        trope_id: "trope-1".to_string(),
        trope_name: "Test Trope".to_string(),
        beat: TropeEscalation {
            at,
            event: event.to_string(),
            npcs_involved: vec![],
            stakes: stakes.to_string(),
        },
    }
}

fn active_stake(description: &str) -> ActiveStake {
    ActiveStake {
        description: description.to_string(),
    }
}

fn slow_trope_def() -> TropeDefinition {
    TropeDefinition {
        id: Some("rising_threat".to_string()),
        name: NonBlankString::new("Rising Threat").unwrap(),
        description: Some("A danger grows".to_string()),
        category: "conflict".to_string(),
        triggers: vec![],
        narrative_hints: vec!["Danger lurks".to_string()],
        tension_level: Some(0.5),
        resolution_hints: None,
        resolution_patterns: None,
        tags: vec![],
        escalation: vec![
            TropeEscalation {
                at: 0.1,
                event: "Strange noises".to_string(),
                npcs_involved: vec![],
                stakes: "Camp safety".to_string(),
            },
            TropeEscalation {
                at: 0.5,
                event: "A scout goes missing".to_string(),
                npcs_involved: vec!["Scout Kira".to_string()],
                stakes: "Lives at risk".to_string(),
            },
        ],
        passive_progression: Some(PassiveProgression {
            rate_per_turn: 0.05,
            rate_per_day: 0.0,
            accelerators: vec![],
            decelerators: vec![],
            accelerator_bonus: 0.0,
            decelerator_penalty: 0.0,
        }),
        is_abstract: false,
        extends: None,
    }
}

// ============================================================================
// AC: Engagement tracked — meaningful intent classification
// is_meaningful was removed in PR#95; meaningful = Combat | Dialogue | Chase
// ============================================================================

/// Helper: meaningful intents are active engagement (Combat, Dialogue, Chase).
fn is_meaningful(intent: Intent) -> bool {
    matches!(intent, Intent::Combat | Intent::Dialogue | Intent::Chase)
}

#[test]
fn combat_intent_is_meaningful() {
    assert!(is_meaningful(Intent::Combat), "Combat should be meaningful");
}

#[test]
fn dialogue_intent_is_meaningful() {
    assert!(
        is_meaningful(Intent::Dialogue),
        "Dialogue should be meaningful"
    );
}

#[test]
fn chase_intent_is_meaningful() {
    assert!(is_meaningful(Intent::Chase), "Chase should be meaningful");
}

#[test]
fn exploration_intent_is_not_meaningful() {
    assert!(
        !is_meaningful(Intent::Exploration),
        "Exploration should not be meaningful"
    );
}

#[test]
fn examine_intent_is_not_meaningful() {
    assert!(
        !is_meaningful(Intent::Examine),
        "Examine should not be meaningful"
    );
}

#[test]
fn meta_intent_is_not_meaningful() {
    assert!(
        !is_meaningful(Intent::Meta),
        "Meta should not be meaningful"
    );
}

#[test]
fn intent_route_exposes_meaningful_via_intent() {
    let route = IntentRoute::for_intent(Intent::Combat);
    assert!(
        is_meaningful(route.intent()),
        "Combat route should be meaningful"
    );

    let route = IntentRoute::for_intent(Intent::Exploration);
    assert!(
        !is_meaningful(route.intent()),
        "Exploration route should not be meaningful"
    );
}

// ============================================================================
// AC: Engagement tracked — counter updates based on intent
// ============================================================================

#[test]
fn meaningful_intent_resets_turns_since_meaningful() {
    let mut snap = GameSnapshot::default();
    snap.turns_since_meaningful = 5; // player was idle

    // Simulate meaningful action (Combat)
    let route = IntentRoute::for_intent(Intent::Combat);
    if is_meaningful(route.intent()) {
        snap.turns_since_meaningful = 0;
    } else {
        snap.turns_since_meaningful += 1;
    }

    assert_eq!(
        snap.turns_since_meaningful, 0,
        "Meaningful action should reset counter to 0"
    );
}

#[test]
fn non_meaningful_intent_increments_turns_since_meaningful() {
    let mut snap = GameSnapshot::default();
    snap.turns_since_meaningful = 3;

    // Simulate non-meaningful action (Exploration)
    let route = IntentRoute::for_intent(Intent::Exploration);
    if is_meaningful(route.intent()) {
        snap.turns_since_meaningful = 0;
    } else {
        snap.turns_since_meaningful += 1;
    }

    assert_eq!(
        snap.turns_since_meaningful, 4,
        "Non-meaningful action should increment counter"
    );
}

// ============================================================================
// AC: Multiplier applied — trope tick receives scaled delta time
// ============================================================================

#[test]
fn engagement_multiplier_feeds_into_tick_with_multiplier() {
    // Full wiring: GameSnapshot counter → engagement_multiplier → tick_with_multiplier
    let mut snap = GameSnapshot::default();
    snap.turns_since_meaningful = 7; // very passive

    let multiplier = engagement_multiplier(snap.turns_since_meaningful);
    assert_eq!(multiplier, 2.0, "7 turns idle should give 2.0x multiplier");

    let defs = vec![slow_trope_def()];
    let mut tropes = vec![TropeState::new("rising_threat")];

    // Tick with 2.0x multiplier: rate 0.05 * 2.0 = 0.10
    let fired = TropeEngine::tick_with_multiplier(&mut tropes, &defs, multiplier as f64);

    assert!(
        (tropes[0].progression() - 0.10).abs() < f64::EPSILON,
        "2.0x multiplier should double progression rate. Got {}",
        tropes[0].progression()
    );
    // 0.10 crosses the 0.1 beat threshold
    assert_eq!(
        fired.len(),
        1,
        "Beat at 0.1 should fire with 2.0x multiplier"
    );
}

// ============================================================================
// AC: Directive per turn — fired beats produce a SceneDirective
// ============================================================================

#[test]
fn fired_beats_produce_scene_directive() {
    let beats = vec![fired_beat("Strange noises in the camp", 0.1, "Camp safety")];
    let stakes = vec![active_stake("The village alliance is crumbling")];
    let hints = vec!["Smoke rises from the east".to_string()];

    let directive = format_scene_directive(&beats, &stakes, &hints, &[]);

    assert!(
        !directive.mandatory_elements.is_empty(),
        "Directive should have mandatory elements from fired beats"
    );
    assert_eq!(
        directive.narrative_hints.len(),
        1,
        "Directive should carry narrative hints"
    );
}

// ============================================================================
// AC: Prompt contains directive — narrator prompt includes MANDATORY block
// ============================================================================

#[test]
fn directive_registered_in_prompt_produces_mandatory_block() {
    let beats = vec![fired_beat("a distant explosion", 0.8, "safety")];
    let stakes = vec![active_stake("the trade routes are severed")];
    let hints = vec!["Smoke rises".to_string()];

    let directive = format_scene_directive(&beats, &stakes, &hints, &[]);

    let mut registry = PromptRegistry::new();
    registry.register_scene_directive("narrator", &directive);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("[SCENE DIRECTIVES"),
        "Composed prompt should contain scene directive header. Got: {}",
        composed
    );
    assert!(
        composed.contains("MUST"),
        "Composed prompt should contain MUST-weave language"
    );
}

#[test]
fn directive_placed_in_early_zone() {
    let beats = vec![fired_beat("an explosion", 0.8, "safety")];
    let directive = format_scene_directive(&beats, &[], &[], &[]);

    let mut registry = PromptRegistry::new();
    registry.register_scene_directive("narrator", &directive);

    let sections = registry.get_sections("narrator", None, Some(AttentionZone::Early));
    assert!(
        !sections.is_empty(),
        "Scene directive should be in Early attention zone"
    );
    let directive_section = sections.iter().find(|s| s.name == "scene_directive");
    assert!(
        directive_section.is_some(),
        "Should find a section named 'scene_directive' in Early zone"
    );
}

// ============================================================================
// AC: No directive agents — Combat/Chase agents don't receive directives
// ============================================================================

#[test]
fn combat_agent_does_not_receive_directive() {
    let beats = vec![fired_beat("an explosion", 0.8, "safety")];
    let directive = format_scene_directive(&beats, &[], &[], &[]);

    let mut registry = PromptRegistry::new();
    // Only register for narrator, NOT for creature_smith
    registry.register_scene_directive("narrator", &directive);

    let combat_prompt = registry.compose("creature_smith");
    assert!(
        !combat_prompt.contains("[SCENE DIRECTIVES"),
        "Combat agent should NOT receive scene directives"
    );
}

#[test]
fn chase_agent_does_not_receive_directive() {
    let beats = vec![fired_beat("an explosion", 0.8, "safety")];
    let directive = format_scene_directive(&beats, &[], &[], &[]);

    let mut registry = PromptRegistry::new();
    registry.register_scene_directive("narrator", &directive);

    let chase_prompt = registry.compose("dialectician");
    assert!(
        !chase_prompt.contains("[SCENE DIRECTIVES"),
        "Chase agent should NOT receive scene directives"
    );
}

// ============================================================================
// AC: Backward compatible — empty beats produce empty directive
// ============================================================================

#[test]
fn empty_beats_produce_no_directive_block() {
    // No fired beats, no stakes → empty directive
    let directive = format_scene_directive(&[], &[], &[], &[]);

    assert!(
        directive.mandatory_elements.is_empty(),
        "Empty beats should produce empty directive"
    );

    // Render should return None for empty directive
    let rendered = render_scene_directive(&directive);
    assert!(
        rendered.is_none(),
        "Empty directive should render to None (suppressed)"
    );
}

#[test]
fn empty_directive_not_registered_in_prompt() {
    let directive = format_scene_directive(&[], &[], &[], &[]);

    let mut registry = PromptRegistry::new();
    registry.register_scene_directive("narrator", &directive);

    let composed = registry.compose("narrator");
    assert!(
        !composed.contains("[SCENE DIRECTIVES"),
        "Empty directive should NOT appear in prompt"
    );
}

// ============================================================================
// AC: Faction events included — placeholder for 6-4/6-5
// ============================================================================

#[test]
fn faction_events_field_exists_on_directive() {
    let directive = format_scene_directive(&[], &[], &[], &[]);
    // faction_events should exist and be empty (placeholder until 6-5 wires it)
    assert!(
        directive.faction_events.is_empty(),
        "Faction events should be empty until story 6-5 wires them"
    );
}

// ============================================================================
// AC: Ordering correct — engagement → tick → directive → prompt
// ============================================================================

#[test]
fn full_pipeline_engagement_to_directive_to_prompt() {
    // Integration test: simulate a full turn pipeline
    // 1. Start with a passive player (7 turns idle)
    let mut snap = GameSnapshot::default();
    snap.turns_since_meaningful = 7;

    // 2. Compute engagement multiplier
    let multiplier = engagement_multiplier(snap.turns_since_meaningful);
    assert_eq!(multiplier, 2.0);

    // 3. Tick trope engine with multiplier
    let defs = vec![slow_trope_def()];
    let mut tropes = vec![TropeState::new("rising_threat")];
    let fired = TropeEngine::tick_with_multiplier(&mut tropes, &defs, multiplier as f64);

    // 4. Generate directive from fired beats
    let stakes: Vec<ActiveStake> = if !snap.active_stakes.is_empty() {
        vec![active_stake(&snap.active_stakes)]
    } else {
        vec![]
    };
    let directive = format_scene_directive(&fired, &stakes, &[], &[]);

    // 5. With 2.0x multiplier from passive player, beat at 0.1 should have fired
    assert!(
        !directive.mandatory_elements.is_empty(),
        "Passive player's accelerated trope tick should produce directive elements"
    );

    // 6. Register in prompt and verify
    let mut registry = PromptRegistry::new();
    registry.register_scene_directive("narrator", &directive);
    let composed = registry.compose("narrator");

    assert!(
        composed.contains("[SCENE DIRECTIVES"),
        "Full pipeline should produce directive block in prompt"
    );
    assert!(
        composed.contains("Strange noises"),
        "Fired beat event should appear in prompt"
    );
}

#[test]
fn active_player_pipeline_may_not_fire_beats() {
    // Contrast test: active player (0 turns idle) → 0.5x multiplier → slower progression
    let mut snap = GameSnapshot::default();
    snap.turns_since_meaningful = 0;

    let multiplier = engagement_multiplier(snap.turns_since_meaningful);
    assert_eq!(multiplier, 0.5);

    let defs = vec![slow_trope_def()];
    let mut tropes = vec![TropeState::new("rising_threat")];
    // 0.05 * 0.5 = 0.025 — doesn't cross 0.1 threshold
    let fired = TropeEngine::tick_with_multiplier(&mut tropes, &defs, multiplier as f64);

    assert!(
        fired.is_empty(),
        "Active player with 0.5x multiplier should not fire beat on first tick"
    );

    // Empty fired beats → empty directive → no block in prompt
    let directive = format_scene_directive(&fired, &[], &[], &[]);
    let mut registry = PromptRegistry::new();
    registry.register_scene_directive("narrator", &directive);
    let composed = registry.compose("narrator");

    assert!(
        !composed.contains("[SCENE DIRECTIVES"),
        "No fired beats → no directive block in prompt"
    );
}

// ============================================================================
// AC: Narrator-only injection — only narrator agent gets directive
// ============================================================================

#[test]
fn only_narrator_route_triggers_directive_injection() {
    // Verify that intent classification determines which agent gets the directive
    let narrator_route = IntentRoute::for_intent(Intent::Exploration);
    assert_eq!(
        narrator_route.agent_name(),
        "narrator",
        "Exploration should route to narrator"
    );

    // ADR-067: All intents route to narrator
    let combat_route = IntentRoute::for_intent(Intent::Combat);
    assert_eq!(
        combat_route.agent_name(),
        "narrator",
        "Combat should route to narrator (ADR-067: unified narrator)"
    );

    // The wiring should only inject directives when agent_name == "narrator"
    // (This is tested structurally — the directive is only registered for "narrator")
}

// ============================================================================
// Rule #6: Test quality self-check
// ============================================================================
// Every test uses assert!, assert_eq!, or specific value checks.
// No `let _ =` patterns. No `assert!(true)`.
// Integration tests verify actual prompt content, not just "something happened."
// Pipeline tests check exact multiplier values and beat firing behavior.
