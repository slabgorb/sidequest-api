//! RED tests for Story 13-14: Sealed-round prompt architecture.
//!
//! The sealed-letter system collects all player actions behind a barrier, then
//! resolves them in ONE narrator call with initiative context. This test file
//! covers the prompt composition and barrier-side behavior.
//!
//! Types under test:
//!   - `SealedRoundContext` — prompt section with all actions + initiative
//!   - `TurnBarrier::named_actions()` — no submission-order bias
//!   - `TurnBarrier::build_sealed_round_context()` — compose full prompt section
//!
//! These tests WILL NOT COMPILE until the types are created — RED state for TDD.

use std::collections::HashMap;

use sidequest_game::barrier::{TurnBarrier, TurnBarrierConfig};
use sidequest_game::multiplayer::MultiplayerSession;
use sidequest_game::sealed_round::{SealedRoundContext, build_sealed_round_context};
use sidequest_genre::InitiativeRule;

// ===========================================================================
// Fixtures
// ===========================================================================

fn sample_actions() -> HashMap<String, String> {
    let mut actions = HashMap::new();
    actions.insert("Kael".to_string(), "I attack the goblin".to_string());
    actions.insert("Lyra".to_string(), "I cast shield on Kael".to_string());
    actions.insert("Thane".to_string(), "I guard the door".to_string());
    actions
}

fn sample_initiative_rules() -> HashMap<String, InitiativeRule> {
    let mut rules = HashMap::new();
    rules.insert(
        "combat".to_string(),
        InitiativeRule {
            primary_stat: "DEX".to_string(),
            description: "Reflexes and speed determine who strikes first".to_string(),
        },
    );
    rules.insert(
        "social".to_string(),
        InitiativeRule {
            primary_stat: "CHA".to_string(),
            description: "Force of personality controls the conversation".to_string(),
        },
    );
    rules
}

fn sample_player_stats() -> HashMap<String, HashMap<String, i32>> {
    let mut stats = HashMap::new();
    stats.insert("Kael".to_string(), {
        let mut s = HashMap::new();
        s.insert("DEX".to_string(), 16);
        s.insert("CHA".to_string(), 10);
        s
    });
    stats.insert("Lyra".to_string(), {
        let mut s = HashMap::new();
        s.insert("DEX".to_string(), 12);
        s.insert("CHA".to_string(), 14);
        s
    });
    stats.insert("Thane".to_string(), {
        let mut s = HashMap::new();
        s.insert("DEX".to_string(), 14);
        s.insert("CHA".to_string(), 8);
        s
    });
    stats
}

// ===========================================================================
// AC: Actions unordered in prompt — no submission-time bias
// ===========================================================================

#[test]
fn sealed_round_context_contains_all_actions() {
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    let prompt = ctx.to_prompt_section();
    assert!(prompt.contains("Kael"), "prompt must include Kael's action");
    assert!(prompt.contains("Lyra"), "prompt must include Lyra's action");
    assert!(prompt.contains("Thane"), "prompt must include Thane's action");
    assert!(
        prompt.contains("I attack the goblin"),
        "prompt must include Kael's action text"
    );
    assert!(
        prompt.contains("I cast shield on Kael"),
        "prompt must include Lyra's action text"
    );
    assert!(
        prompt.contains("I guard the door"),
        "prompt must include Thane's action text"
    );
}

#[test]
fn sealed_round_context_has_simultaneous_instruction() {
    // The prompt MUST tell the narrator that actions were submitted simultaneously
    // and no player knew what others chose.
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    let prompt = ctx.to_prompt_section();
    assert!(
        prompt.contains("simultaneous") || prompt.contains("Simultaneous"),
        "prompt must state actions were submitted simultaneously"
    );
}

#[test]
fn sealed_round_context_actions_not_ordered_by_name() {
    // Actions should be presented as a SET, not sorted. Run multiple times
    // and verify it doesn't always produce the same order (probabilistic test).
    // Since HashMap iteration is non-deterministic, we verify the prompt
    // does NOT explicitly number actions or use ordered language.
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    let prompt = ctx.to_prompt_section();
    // Should NOT contain numbered list (1. 2. 3.) which implies order
    assert!(
        !prompt.contains("1.") || !prompt.contains("2."),
        "prompt should not use numbered list (implies order)"
    );
}

// ===========================================================================
// AC: Initiative context included — encounter type + stats + genre rules
// ===========================================================================

#[test]
fn sealed_round_context_includes_encounter_type() {
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    let prompt = ctx.to_prompt_section();
    assert!(
        prompt.contains("combat"),
        "prompt must include the encounter type"
    );
}

#[test]
fn sealed_round_context_includes_initiative_rule_description() {
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    let prompt = ctx.to_prompt_section();
    assert!(
        prompt.contains("Reflexes and speed determine who strikes first"),
        "prompt must include the initiative rule description for the encounter type"
    );
}

#[test]
fn sealed_round_context_includes_primary_stat_name() {
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    let prompt = ctx.to_prompt_section();
    assert!(
        prompt.contains("DEX"),
        "prompt must include the primary stat name for initiative"
    );
}

#[test]
fn sealed_round_context_includes_per_player_stat_values() {
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    let prompt = ctx.to_prompt_section();
    // For combat → DEX, we should see each player's DEX value
    assert!(
        prompt.contains("Kael") && prompt.contains("16"),
        "prompt must include Kael's DEX (16)"
    );
    assert!(
        prompt.contains("Lyra") && prompt.contains("12"),
        "prompt must include Lyra's DEX (12)"
    );
    assert!(
        prompt.contains("Thane") && prompt.contains("14"),
        "prompt must include Thane's DEX (14)"
    );
}

#[test]
fn sealed_round_context_includes_initiative_instruction() {
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    let prompt = ctx.to_prompt_section();
    // Must instruct narrator to determine initiative order
    assert!(
        prompt.to_lowercase().contains("initiative")
            && prompt.to_lowercase().contains("order"),
        "prompt must instruct narrator to determine initiative order"
    );
}

#[test]
fn sealed_round_context_with_social_encounter_uses_cha() {
    // Social encounters should reference CHA, not DEX
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "social",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    let prompt = ctx.to_prompt_section();
    assert!(
        prompt.contains("CHA"),
        "social encounter should use CHA for initiative"
    );
    assert!(
        prompt.contains("Force of personality"),
        "social encounter should include CHA rule description"
    );
}

#[test]
fn sealed_round_context_with_unknown_encounter_omits_initiative() {
    // If encounter type isn't in initiative_rules, prompt should still include
    // actions but skip initiative stat context (graceful degradation).
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "puzzle",  // not in our rules
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    let prompt = ctx.to_prompt_section();
    // Actions should still be present
    assert!(prompt.contains("Kael"), "actions still present for unknown encounter");
    // But no specific stat values should be forced
    assert!(
        !prompt.contains("DEX") || !prompt.contains("16"),
        "unknown encounter type should not inject specific stat values"
    );
}

// ===========================================================================
// AC: SealedRoundContext struct fields
// ===========================================================================

#[test]
fn sealed_round_context_has_correct_player_count() {
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    assert_eq!(ctx.player_count(), 3);
}

#[test]
fn sealed_round_context_has_encounter_type() {
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    assert_eq!(ctx.encounter_type(), "combat");
}

#[test]
fn sealed_round_context_roundtrip_action_count() {
    let actions = sample_actions();
    let ctx = build_sealed_round_context(
        &actions,
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    assert_eq!(ctx.action_count(), actions.len());
}

// ===========================================================================
// AC: One narrator call per round — barrier claim + shared narration
// ===========================================================================

#[tokio::test]
async fn barrier_claim_election_yields_one_winner() {
    // When 3 players submit, wait_for_turn resolves for all 3 tasks.
    // Exactly ONE should have claimed_resolution = true.
    let session = MultiplayerSession::with_player_ids(
        vec!["p1".to_string(), "p2".to_string(), "p3".to_string()],
    );
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::disabled());

    barrier.submit_action("p1", "attack");
    barrier.submit_action("p2", "defend");
    barrier.submit_action("p3", "heal");

    // Spawn 3 concurrent wait tasks
    let b1 = barrier.clone();
    let b2 = barrier.clone();
    let b3 = barrier.clone();

    let (r1, r2, r3) = tokio::join!(
        b1.wait_for_turn(),
        b2.wait_for_turn(),
        b3.wait_for_turn(),
    );

    let claimed_count = [r1.claimed_resolution, r2.claimed_resolution, r3.claimed_resolution]
        .iter()
        .filter(|&&c| c)
        .count();

    assert_eq!(
        claimed_count, 1,
        "Exactly one handler should claim resolution (the one that calls narrator)"
    );
}

#[tokio::test]
async fn claimed_handler_can_store_and_others_retrieve_narration() {
    let session = MultiplayerSession::with_player_ids(
        vec!["p1".to_string(), "p2".to_string()],
    );
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::disabled());

    barrier.submit_action("p1", "attack");
    barrier.submit_action("p2", "defend");

    let result = barrier.wait_for_turn().await;
    assert!(result.claimed_resolution);

    // Claiming handler runs narrator and stores result
    barrier.store_resolution_narration("The battle rages...".to_string());

    // Non-claiming handler retrieves the shared narration
    let narration = barrier.get_resolution_narration();
    assert_eq!(
        narration.as_deref(),
        Some("The battle rages..."),
        "Non-claiming handlers should retrieve the stored narration"
    );
}

// ===========================================================================
// AC: Synthesized scene — one scene, not N narrations
// ===========================================================================

#[test]
fn sealed_round_context_has_perspective_directive() {
    // The prompt must tell the narrator to write in third-person omniscient
    // and to produce ONE synthesized scene covering all actions.
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    let prompt = ctx.to_prompt_section();
    assert!(
        prompt.to_lowercase().contains("third-person")
            || prompt.to_lowercase().contains("omniscient"),
        "sealed round prompt must specify third-person or omniscient perspective"
    );
}

#[test]
fn sealed_round_context_has_synthesize_instruction() {
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    let prompt = ctx.to_prompt_section();
    // Must instruct narrator to resolve ALL actions in one scene
    assert!(
        prompt.to_lowercase().contains("all actions")
            || prompt.to_lowercase().contains("resolve")
            || prompt.to_lowercase().contains("synthesize"),
        "prompt must instruct narrator to resolve all actions in one scene"
    );
}

// ===========================================================================
// AC: Barrier named_actions does not leak submission order
// ===========================================================================

#[test]
fn barrier_named_actions_returns_all_submitted() {
    let session = MultiplayerSession::with_player_ids(
        vec!["p1".to_string(), "p2".to_string(), "p3".to_string()],
    );
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::disabled());

    barrier.submit_action("p1", "I attack");
    barrier.submit_action("p2", "I defend");
    barrier.submit_action("p3", "I heal");

    let actions = barrier.named_actions();
    assert_eq!(actions.len(), 3, "All 3 actions should be returned");
}

#[test]
fn barrier_named_actions_uses_character_name_not_player_id() {
    // named_actions keys should be character names, not player IDs
    let session = MultiplayerSession::with_player_ids(
        vec!["p1".to_string(), "p2".to_string()],
    );
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::disabled());

    barrier.submit_action("p1", "I attack");
    barrier.submit_action("p2", "I defend");

    let actions = barrier.named_actions();
    // Keys should be character names (from the multiplayer session's
    // player→character mapping), not raw player IDs like "p1", "p2"
    for key in actions.keys() {
        assert!(
            !key.starts_with('p') || key.len() > 3,
            "named_actions key '{}' looks like a player_id, should be character name",
            key,
        );
    }
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[test]
fn sealed_round_context_with_two_players() {
    let mut actions = HashMap::new();
    actions.insert("Kael".to_string(), "I attack".to_string());
    actions.insert("Lyra".to_string(), "I heal".to_string());

    let ctx = build_sealed_round_context(
        &actions,
        "combat",
        &sample_initiative_rules(),
        &sample_player_stats(),
    );
    assert_eq!(ctx.player_count(), 2);
    let prompt = ctx.to_prompt_section();
    assert!(prompt.contains("Kael"));
    assert!(prompt.contains("Lyra"));
}

#[test]
fn sealed_round_context_with_empty_initiative_rules() {
    // Genre hasn't authored initiative rules — still produces a valid prompt
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &HashMap::new(), // no rules
        &sample_player_stats(),
    );
    let prompt = ctx.to_prompt_section();
    // Actions should still be present
    assert!(prompt.contains("Kael"), "actions present even without initiative rules");
    assert!(prompt.contains("I attack the goblin"));
}

#[test]
fn sealed_round_context_with_missing_player_stats() {
    // Player stats not available — still produce prompt, just without stat values
    let ctx = build_sealed_round_context(
        &sample_actions(),
        "combat",
        &sample_initiative_rules(),
        &HashMap::new(), // no stats
    );
    let prompt = ctx.to_prompt_section();
    assert!(prompt.contains("Kael"), "actions present even without stat data");
}
