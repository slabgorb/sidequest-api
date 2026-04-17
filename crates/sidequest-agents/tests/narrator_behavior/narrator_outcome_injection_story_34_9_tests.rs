//! Story 34-9: Narrator outcome injection — RollOutcome shapes narration tone.
//!
//! Tests verifying that RollOutcome is injected into the narrator prompt as a
//! visible context tag in the mechanical facts zone.
//!
//! ACs tested:
//! 1. Each RollOutcome variant produces distinct [DICE_OUTCOME: X] tag
//! 2. Tag is in the mechanical context zone (Valley attention zone)
//! 3. Unknown outcome is handled gracefully (skip injection, no silent fallback)
//! 4. No outcome (None) produces no tag (backward compat)
//! 5. Other context is unaffected by outcome injection
//! 6. Integration: full prompt path with outcome set

use sidequest_agents::orchestrator::{NarratorPromptTier, Orchestrator, TurnContext};
use sidequest_agents::turn_record::{TurnRecord, WATCHER_CHANNEL_CAPACITY};
use sidequest_protocol::RollOutcome;
use tokio::sync::mpsc;

/// Helper: build an orchestrator with a dummy watcher channel.
fn test_orchestrator() -> Orchestrator {
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    Orchestrator::new(tx)
}

/// Helper: build a TurnContext with a specific RollOutcome.
fn context_with_outcome(outcome: Option<RollOutcome>) -> TurnContext {
    TurnContext {
        roll_outcome: outcome,
        genre: Some("low_fantasy".to_string()),
        ..Default::default()
    }
}

// ============================================================
// AC-1: Each RollOutcome variant produces a distinct tag
// ============================================================

#[test]
fn crit_success_outcome_produces_tag() {
    let orch = test_orchestrator();
    let ctx = context_with_outcome(Some(RollOutcome::CritSuccess));

    let result = orch.build_narrator_prompt("attack the goblin", &ctx);

    assert!(
        result.prompt_text.contains("[DICE_OUTCOME: CritSuccess]"),
        "CritSuccess must produce [DICE_OUTCOME: CritSuccess] tag in prompt, got: {}",
        &result.prompt_text[..result.prompt_text.len().min(2000)]
    );
}

#[test]
fn success_outcome_produces_tag() {
    let orch = test_orchestrator();
    let ctx = context_with_outcome(Some(RollOutcome::Success));

    let result = orch.build_narrator_prompt("pick the lock", &ctx);

    assert!(
        result.prompt_text.contains("[DICE_OUTCOME: Success]"),
        "Success must produce [DICE_OUTCOME: Success] tag in prompt"
    );
}

#[test]
fn fail_outcome_produces_tag() {
    let orch = test_orchestrator();
    let ctx = context_with_outcome(Some(RollOutcome::Fail));

    let result = orch.build_narrator_prompt("leap across the chasm", &ctx);

    assert!(
        result.prompt_text.contains("[DICE_OUTCOME: Fail]"),
        "Fail must produce [DICE_OUTCOME: Fail] tag in prompt"
    );
}

#[test]
fn crit_fail_outcome_produces_tag() {
    let orch = test_orchestrator();
    let ctx = context_with_outcome(Some(RollOutcome::CritFail));

    let result = orch.build_narrator_prompt("disarm the trap", &ctx);

    assert!(
        result.prompt_text.contains("[DICE_OUTCOME: CritFail]"),
        "CritFail must produce [DICE_OUTCOME: CritFail] tag in prompt"
    );
}

// ============================================================
// AC-3: Unknown outcome skips injection (no silent fallback)
// ============================================================

#[test]
fn unknown_outcome_skips_injection() {
    let orch = test_orchestrator();
    let ctx = context_with_outcome(Some(RollOutcome::Unknown));

    let result = orch.build_narrator_prompt("do something", &ctx);

    // Unknown means the wire protocol sent an unrecognized variant.
    // Per project rule "No Silent Fallbacks" and RollOutcome::Unknown docs:
    // the correct response is to skip injection entirely, not to inject
    // misleading neutral narration guidance. The narrator should not receive
    // tone shaping for a mechanically undefined outcome.
    assert!(
        !result.prompt_text.contains("[DICE_OUTCOME:"),
        "Unknown outcome must NOT inject any [DICE_OUTCOME:] tag — silent fallback violates project rules. Got prompt containing DICE_OUTCOME tag."
    );
}

// ============================================================
// AC-4: No outcome (None) produces no dice outcome tag
// ============================================================

#[test]
fn no_outcome_produces_no_tag() {
    let orch = test_orchestrator();
    let ctx = context_with_outcome(None);

    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        !result.prompt_text.contains("[DICE_OUTCOME:"),
        "When roll_outcome is None, no [DICE_OUTCOME:] tag should appear in prompt"
    );
}

// ============================================================
// AC-5: Other context unaffected by outcome injection
// ============================================================

#[test]
fn outcome_injection_does_not_affect_genre_section() {
    let orch = test_orchestrator();

    // With outcome
    let ctx_with = context_with_outcome(Some(RollOutcome::CritSuccess));
    let result_with = orch.build_narrator_prompt("attack", &ctx_with);

    // Without outcome
    let ctx_without = context_with_outcome(None);
    let result_without = orch.build_narrator_prompt("attack", &ctx_without);

    // Genre identity must appear in both
    assert!(
        result_with.prompt_text.contains("<genre>"),
        "Genre section must still be present when outcome is injected"
    );
    assert!(
        result_without.prompt_text.contains("<genre>"),
        "Genre section must be present when no outcome"
    );
}

#[test]
fn outcome_injection_does_not_affect_verbosity_section() {
    let orch = test_orchestrator();
    let ctx = context_with_outcome(Some(RollOutcome::Fail));

    let result = orch.build_narrator_prompt("flee", &ctx);

    // Verbosity section should still be present
    assert!(
        result.prompt_text.contains("<length-limit>"),
        "Verbosity section must still be present when outcome is injected"
    );
}

// ============================================================
// AC-2: Tag placement in mechanical context zone (Valley)
// ============================================================

#[test]
fn outcome_tag_is_in_valley_zone() {
    let orch = test_orchestrator();
    let ctx = context_with_outcome(Some(RollOutcome::Success));

    let result = orch.build_narrator_prompt_tiered("attack", &ctx, NarratorPromptTier::Full);

    // The zone breakdown should show the dice_outcome section in the Valley zone
    let valley_sections: Vec<&str> = result
        .zone_breakdown
        .zones
        .iter()
        .filter(|z| z.zone == sidequest_agents::prompt_framework::AttentionZone::Valley)
        .flat_map(|z| z.sections.iter().map(|s| s.name.as_str()))
        .collect();

    assert!(
        valley_sections.contains(&"dice_outcome"),
        "dice_outcome section must be in Valley zone (mechanical facts), found Valley sections: {:?}",
        valley_sections
    );
}

// ============================================================
// AC-6: Integration — both Full and Delta tiers inject outcome
// ============================================================

#[test]
fn outcome_injected_on_full_tier() {
    let orch = test_orchestrator();
    let ctx = context_with_outcome(Some(RollOutcome::CritFail));

    let result = orch.build_narrator_prompt_tiered("attack", &ctx, NarratorPromptTier::Full);

    assert!(
        result.prompt_text.contains("[DICE_OUTCOME: CritFail]"),
        "Outcome tag must appear on Full tier"
    );
}

#[test]
fn outcome_injected_on_delta_tier() {
    let orch = test_orchestrator();
    let ctx = context_with_outcome(Some(RollOutcome::CritSuccess));

    let result = orch.build_narrator_prompt_tiered("attack", &ctx, NarratorPromptTier::Delta);

    assert!(
        result.prompt_text.contains("[DICE_OUTCOME: CritSuccess]"),
        "Outcome tag must appear on Delta tier (dice results are per-turn, not static)"
    );
}

// ============================================================
// Distinctness: all four known variants produce different tags
// ============================================================

#[test]
fn all_outcome_variants_produce_distinct_tags() {
    let orch = test_orchestrator();
    let variants = [
        (RollOutcome::CritSuccess, "[DICE_OUTCOME: CritSuccess]"),
        (RollOutcome::Success, "[DICE_OUTCOME: Success]"),
        (RollOutcome::Fail, "[DICE_OUTCOME: Fail]"),
        (RollOutcome::CritFail, "[DICE_OUTCOME: CritFail]"),
    ];

    for (outcome, expected_tag) in &variants {
        let ctx = context_with_outcome(Some(*outcome));
        let result = orch.build_narrator_prompt("test action", &ctx);

        assert!(
            result.prompt_text.contains(expected_tag),
            "Outcome {:?} must produce tag '{}' in prompt",
            outcome,
            expected_tag,
        );

        // Verify ONLY this variant's tag appears (not others)
        for (other_outcome, other_tag) in &variants {
            if other_outcome != outcome {
                assert!(
                    !result.prompt_text.contains(other_tag),
                    "Outcome {:?} must NOT contain tag for {:?}",
                    outcome,
                    other_outcome,
                );
            }
        }
    }
}

// ============================================================
// Prompt stability: same outcome produces same tag deterministically
// ============================================================

#[test]
fn same_outcome_produces_identical_prompt_tag() {
    let orch = test_orchestrator();
    let ctx = context_with_outcome(Some(RollOutcome::Success));

    let result1 = orch.build_narrator_prompt("attack", &ctx);
    let result2 = orch.build_narrator_prompt("attack", &ctx);

    // Extract the dice outcome section from both prompts
    let tag = "[DICE_OUTCOME: Success]";
    assert!(result1.prompt_text.contains(tag));
    assert!(result2.prompt_text.contains(tag));

    // Same inputs, same prompt — deterministic
    assert_eq!(
        result1.prompt_text, result2.prompt_text,
        "Same outcome + same action must produce identical prompts"
    );
}

// ============================================================
// Wiring test: TurnContext.roll_outcome field exists and is used
// ============================================================

#[test]
fn turn_context_has_roll_outcome_field() {
    // Compile-time test: TurnContext must have a roll_outcome field of type Option<RollOutcome>
    let ctx = TurnContext {
        roll_outcome: Some(RollOutcome::CritSuccess),
        ..Default::default()
    };
    assert_eq!(ctx.roll_outcome, Some(RollOutcome::CritSuccess));
}

#[test]
fn turn_context_default_has_no_outcome() {
    let ctx = TurnContext::default();
    assert_eq!(
        ctx.roll_outcome, None,
        "Default TurnContext must have roll_outcome = None (backward compat)"
    );
}
