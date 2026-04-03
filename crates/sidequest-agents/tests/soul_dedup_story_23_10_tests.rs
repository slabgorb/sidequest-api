//! Story 23-10: Deduplicate SOUL overlap — Agency and Genre Truth double-injection
//!
//! RED phase — tests that verify Agency and Genre Truth/Consequences concepts
//! appear exactly once in the narrator prompt, not twice.
//!
//! The overlap: narrator_agency (Primacy) duplicates SOUL "Agency" (Early),
//! narrator_consequences (Primacy) duplicates SOUL "Genre Truth" (Early).
//!
//! ACs covered:
//!   AC-1: Agency appears exactly once in narrator prompt
//!   AC-2: Genre Truth / Consequences appears exactly once in narrator prompt
//!   AC-3: Non-narrator agents still receive Agency and Genre Truth from SOUL
//!   AC-4: Total narrator prompt token count reduced
//!   AC-6: OTEL zone_distribution reflects reduced Early/Soul section

use sidequest_agents::orchestrator::{Orchestrator, TurnContext};
use sidequest_agents::prompt_framework::soul::{SoulData, SoulPrinciple};
use sidequest_agents::turn_record::{TurnRecord, WATCHER_CHANNEL_CAPACITY};
use tokio::sync::mpsc;

// ============================================================================
// Test helpers
// ============================================================================

/// Build a SoulData with Agency and Genre Truth tagged for all agents
/// (the current pre-fix state).
fn soul_with_all_tags() -> SoulData {
    SoulData {
        principles: vec![
            SoulPrinciple {
                name: "Agency".to_string(),
                text: "The player controls their character.".to_string(),
                agents: vec!["all".to_string()],
            },
            SoulPrinciple {
                name: "Living World".to_string(),
                text: "NPCs act on their own goals.".to_string(),
                agents: vec!["all".to_string()],
            },
            SoulPrinciple {
                name: "Genre Truth".to_string(),
                text: "Consequences follow the genre pack's tone.".to_string(),
                agents: vec!["all".to_string()],
            },
            SoulPrinciple {
                name: "Rule of Cool".to_string(),
                text: "Lean toward allowing creative actions.".to_string(),
                agents: vec!["all".to_string()],
            },
        ],
        title: Some("SOUL".to_string()),
        description: None,
    }
}

fn context_with_genre(genre: &str) -> TurnContext {
    TurnContext {
        genre: Some(genre.to_string()),
        ..Default::default()
    }
}

// ============================================================================
// AC-1: Agency appears exactly once in narrator prompt
// ============================================================================

/// The narrator prompt must contain the Agency concept exactly once.
/// Currently it appears twice: narrator_agency (Primacy) AND SOUL Agency (Early).
///
/// RED because: SOUL "Agency" is tagged `<agents>all</agents>` so it passes
/// the narrator filter and gets injected alongside narrator_agency.
#[test]
fn narrator_prompt_has_agency_exactly_once() {
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let mut orch = Orchestrator::new(tx);
    // Inject SOUL data with Agency tagged for all agents
    orch.set_soul_data(soul_with_all_tags());

    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    // Count occurrences of "Agency" as a section concept.
    // The narrator_agency guardrail starts with "Agency: The player controls..."
    // The SOUL Agency principle would add "- Agency: The player controls..."
    // After dedup, only the narrator version should remain.
    let agency_count = result
        .prompt_text
        .matches("Agency:")
        .count();

    assert_eq!(
        agency_count, 1,
        "Agency concept should appear exactly once in narrator prompt, found {agency_count} occurrences"
    );
}

// ============================================================================
// AC-2: Genre Truth / Consequences appears exactly once
// ============================================================================

/// The narrator prompt must contain Genre Truth / Consequences exactly once.
/// Currently narrator_consequences (Primacy) AND SOUL "Genre Truth" (Early) both appear.
///
/// RED because: SOUL "Genre Truth" is tagged `<agents>all</agents>`.
#[test]
fn narrator_prompt_has_genre_truth_exactly_once() {
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let mut orch = Orchestrator::new(tx);
    orch.set_soul_data(soul_with_all_tags());

    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    // The narrator_consequences text starts with "Consequences follow the genre pack's tone"
    // The SOUL "Genre Truth" text also says "Consequences follow the genre pack's tone"
    // After dedup, only one should remain.
    let genre_truth_count = result
        .prompt_text
        .matches("Consequences follow the genre pack")
        .count();

    assert_eq!(
        genre_truth_count, 1,
        "Genre Truth/Consequences should appear exactly once in narrator prompt, found {genre_truth_count}"
    );
}

// ============================================================================
// AC-3: Non-narrator agents still receive Agency and Genre Truth from SOUL
// ============================================================================

/// SOUL filtering for non-narrator agents must still include Agency.
///
/// RED because: This tests the SOUL data filtering, not the orchestrator.
/// After the fix, Agency should be excluded from narrator but included for others.
#[test]
fn soul_includes_agency_for_ensemble() {
    let soul = soul_with_all_tags();
    let text = soul.as_prompt_text_for("ensemble");

    assert!(
        text.contains("Agency:"),
        "Ensemble agent should still receive Agency from SOUL"
    );
}

#[test]
fn soul_includes_genre_truth_for_creature_smith() {
    let soul = soul_with_all_tags();
    let text = soul.as_prompt_text_for("creature_smith");

    assert!(
        text.contains("Genre Truth:"),
        "CreatureSmith agent should still receive Genre Truth from SOUL"
    );
}

/// After dedup, SOUL filtering for narrator should exclude Agency and Genre Truth.
///
/// RED because: `as_prompt_text_for("narrator")` currently returns Agency and
/// Genre Truth since they're tagged `all`.
#[test]
fn soul_excludes_agency_for_narrator() {
    let soul = soul_with_all_tags();
    let text = soul.as_prompt_text_for("narrator");

    assert!(
        !text.contains("Agency:"),
        "Narrator should NOT receive Agency from SOUL (it has narrator_agency guardrail)"
    );
}

#[test]
fn soul_excludes_genre_truth_for_narrator() {
    let soul = soul_with_all_tags();
    let text = soul.as_prompt_text_for("narrator");

    assert!(
        !text.contains("Genre Truth:"),
        "Narrator should NOT receive Genre Truth from SOUL (it has narrator_consequences guardrail)"
    );
}

/// Non-overlapping SOUL principles must still be included for narrator.
#[test]
fn soul_includes_non_overlapping_for_narrator() {
    let soul = soul_with_all_tags();
    let text = soul.as_prompt_text_for("narrator");

    assert!(
        text.contains("Living World:"),
        "Narrator should still receive Living World from SOUL (no overlap)"
    );
    assert!(
        text.contains("Rule of Cool:"),
        "Narrator should still receive Rule of Cool from SOUL (no overlap)"
    );
}

// ============================================================================
// AC-4: Token reduction in narrator prompt
// ============================================================================

/// The narrator SOUL section should be shorter after dedup (Agency + Genre Truth removed).
/// Pre-dedup: ~8 principles for narrator. Post-dedup: ~6 principles (2 removed).
///
/// RED because: Currently all 8 SOUL principles pass the narrator filter.
#[test]
fn narrator_soul_section_has_fewer_principles_after_dedup() {
    let soul = soul_with_all_tags();
    let narrator_text = soul.as_prompt_text_for("narrator");
    let ensemble_text = soul.as_prompt_text_for("ensemble");

    // Narrator should have fewer SOUL principles than ensemble
    // because Agency and Genre Truth are excluded from narrator
    let narrator_count = narrator_text.lines().count();
    let ensemble_count = ensemble_text.lines().count();

    assert!(
        narrator_count < ensemble_count,
        "Narrator should have fewer SOUL principles ({narrator_count}) than ensemble ({ensemble_count}) after dedup"
    );
}

// ============================================================================
// Wiring: end-to-end integration test
// ============================================================================

/// Full pipeline: Orchestrator with SOUL data → build narrator prompt →
/// verify Agency appears once, Genre Truth appears once, non-overlapping principles present.
#[test]
fn wiring_soul_dedup_end_to_end() {
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let mut orch = Orchestrator::new(tx);
    orch.set_soul_data(soul_with_all_tags());

    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    // Agency: exactly once (narrator_agency guardrail in Primacy, not SOUL in Early)
    let agency_count = result.prompt_text.matches("Agency:").count();
    assert_eq!(
        agency_count, 1,
        "Wiring: Agency should appear exactly once, got {agency_count}"
    );

    // Genre Truth/Consequences: exactly once
    let consequences_count = result
        .prompt_text
        .matches("Consequences follow the genre pack")
        .count();
    assert_eq!(
        consequences_count, 1,
        "Wiring: Consequences should appear exactly once, got {consequences_count}"
    );

    // Non-overlapping SOUL principles should still be present
    assert!(
        result.prompt_text.contains("Living World"),
        "Wiring: Living World SOUL principle should be in narrator prompt"
    );
    assert!(
        result.prompt_text.contains("Rule of Cool"),
        "Wiring: Rule of Cool SOUL principle should be in narrator prompt"
    );

    // The narrator guardrails should be present (they have richer content)
    assert!(
        result.prompt_text.contains("multiplayer"),
        "Wiring: narrator_agency should include multiplayer rules"
    );
    assert!(
        result.prompt_text.contains("NPCs fight for their lives"),
        "Wiring: narrator_consequences should include NPC tactical behavior"
    );
}

/// Negative case: with no SOUL data, narrator prompt should still work normally.
#[test]
fn narrator_works_without_soul_data() {
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    // narrator_agency should still be present (from narrator.rs, not SOUL)
    assert!(
        result.prompt_text.contains("Agency:"),
        "Narrator agency guardrail should be present even without SOUL data"
    );
}
