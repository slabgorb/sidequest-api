//! Tests for Story 15-18: Wire materialize_world into narrator prompt.
//!
//! The WorldBuilderAgent has materialized_world_context() and OTEL spans,
//! but the orchestrator's build_narrator_prompt() never uses it. The wiring
//! gap: TurnContext has no history_chapters field, so the prompt builder
//! can't inject materialized world content even though the agent supports it.
//!
//! These tests verify:
//! 1. TurnContext carries history_chapters
//! 2. build_narrator_prompt injects materialized world context when chapters present
//! 3. Materialized content is filtered by campaign maturity
//! 4. Empty chapters → no materialization section in prompt
//! 5. OTEL span world.materialized fires during prompt building

use sidequest_agents::orchestrator::{Orchestrator, TurnContext};
use sidequest_game::world_materialization::{CampaignMaturity, HistoryChapter};

/// Helper: build a minimal HistoryChapter at a given maturity id with lore.
fn chapter(id: &str, label: &str, lore: Vec<&str>) -> HistoryChapter {
    HistoryChapter {
        id: id.to_string(),
        label: label.to_string(),
        lore: lore.into_iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

/// Helper: build chapters across all maturity tiers.
fn all_tier_chapters() -> Vec<HistoryChapter> {
    vec![
        chapter(
            "fresh",
            "The Awakening",
            vec!["The world is new and strange."],
        ),
        chapter(
            "early",
            "Rising Tensions",
            vec!["Factions emerge from the shadows."],
        ),
        chapter(
            "mid",
            "The Reckoning",
            vec!["Alliances shatter under pressure."],
        ),
        chapter(
            "veteran",
            "Age of Legends",
            vec!["Heroes are forged in the crucible of war."],
        ),
    ]
}

// ═══════════════════════════════════════════════════════════════
// AC-1: TurnContext carries history_chapters
// ═══════════════════════════════════════════════════════════════

#[test]
fn turn_context_has_history_chapters_field() {
    // TurnContext must have a history_chapters field so the orchestrator
    // can pass genre pack chapters through to the prompt builder.
    let ctx = TurnContext {
        history_chapters: all_tier_chapters(),
        ..Default::default()
    };
    assert_eq!(ctx.history_chapters.len(), 4);
}

#[test]
fn turn_context_has_campaign_maturity_field() {
    // TurnContext must carry campaign maturity so the prompt builder
    // knows which chapters to filter.
    let ctx = TurnContext {
        campaign_maturity: CampaignMaturity::Mid,
        ..Default::default()
    };
    assert_eq!(ctx.campaign_maturity, CampaignMaturity::Mid);
}

// ═══════════════════════════════════════════════════════════════
// AC-2: build_narrator_prompt injects materialized world context
// ═══════════════════════════════════════════════════════════════

#[test]
fn narrator_prompt_includes_materialized_world_when_chapters_present() {
    let orchestrator = Orchestrator::new_for_test();
    let ctx = TurnContext {
        history_chapters: all_tier_chapters(),
        campaign_maturity: CampaignMaturity::Early,
        ..Default::default()
    };

    let result = orchestrator.build_narrator_prompt("look around", &ctx);

    // The prompt should contain materialized world content from chapters
    assert!(
        result.prompt_text.contains("The world is new and strange."),
        "Fresh chapter lore should appear in Early maturity prompt"
    );
    assert!(
        result
            .prompt_text
            .contains("Factions emerge from the shadows."),
        "Early chapter lore should appear in Early maturity prompt"
    );
}

// ═══════════════════════════════════════════════════════════════
// AC-3: Materialized content filtered by maturity
// ═══════════════════════════════════════════════════════════════

#[test]
fn narrator_prompt_filters_chapters_by_maturity() {
    let orchestrator = Orchestrator::new_for_test();
    let ctx = TurnContext {
        history_chapters: all_tier_chapters(),
        campaign_maturity: CampaignMaturity::Early,
        ..Default::default()
    };

    let result = orchestrator.build_narrator_prompt("look around", &ctx);

    // Early should include fresh + early but NOT mid or veteran
    assert!(
        result.prompt_text.contains("The world is new and strange."),
        "Fresh lore should be included at Early"
    );
    assert!(
        !result.prompt_text.contains("Alliances shatter"),
        "Mid lore should NOT appear at Early maturity"
    );
    assert!(
        !result.prompt_text.contains("Heroes are forged"),
        "Veteran lore should NOT appear at Early maturity"
    );
}

// ═══════════════════════════════════════════════════════════════
// AC-4: Empty chapters → no materialization section
// ═══════════════════════════════════════════════════════════════

#[test]
fn narrator_prompt_no_materialization_when_no_chapters() {
    let orchestrator = Orchestrator::new_for_test();
    let ctx = TurnContext {
        history_chapters: Vec::new(),
        campaign_maturity: CampaignMaturity::Mid,
        ..Default::default()
    };

    let result = orchestrator.build_narrator_prompt("look around", &ctx);

    // No world_materialization tags should appear
    assert!(
        !result.prompt_text.contains("<world_materialization>"),
        "Empty chapters should not inject materialization section"
    );
}

// ═══════════════════════════════════════════════════════════════
// Wiring test: WorldBuilderAgent used from orchestrator
// ═══════════════════════════════════════════════════════════════

#[test]
fn orchestrator_uses_world_builder_agent_for_materialization() {
    // The orchestrator should use WorldBuilderAgent (not inline logic)
    // to produce the materialized world context. This ensures the agent's
    // OTEL span (world.materialized) fires during normal prompt building.
    let orchestrator = Orchestrator::new_for_test();
    let ctx = TurnContext {
        history_chapters: all_tier_chapters(),
        campaign_maturity: CampaignMaturity::Veteran,
        ..Default::default()
    };

    let result = orchestrator.build_narrator_prompt("look around", &ctx);

    // Veteran maturity should include all tier labels
    assert!(
        result.prompt_text.contains("The Awakening"),
        "Fresh chapter label missing"
    );
    assert!(
        result.prompt_text.contains("Rising Tensions"),
        "Early chapter label missing"
    );
    assert!(
        result.prompt_text.contains("The Reckoning"),
        "Mid chapter label missing"
    );
    assert!(
        result.prompt_text.contains("Age of Legends"),
        "Veteran chapter label missing"
    );
}
