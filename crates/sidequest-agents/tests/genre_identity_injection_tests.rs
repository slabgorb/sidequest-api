//! Genre identity injection tests — fix/playtest-2026-04-05.
//!
//! Root cause: narrator had no genre context in prompt, broke character and asked
//! the player "What genre is Ashgate Square in?"
//!
//! These tests verify:
//! 1. Genre identity is injected into the narrator prompt when present
//! 2. Genre identity appears on both Full and Delta tiers
//! 3. No genre section when genre is None (backward compat)

use sidequest_agents::orchestrator::{NarratorPromptTier, Orchestrator, TurnContext};
use sidequest_agents::turn_record::{TurnRecord, WATCHER_CHANNEL_CAPACITY};
use tokio::sync::mpsc;

#[test]
fn narrator_prompt_contains_genre_identity_when_genre_set() {
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    let ctx = TurnContext {
        genre: Some("low_fantasy".to_string()),
        ..Default::default()
    };

    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        result.prompt_text.contains("<genre>"),
        "Prompt must contain <genre> section when genre is set"
    );
    assert!(
        result.prompt_text.contains("low fantasy"),
        "Genre display name must appear with underscores replaced by spaces, got prompt: {}",
        &result.prompt_text[..result.prompt_text.len().min(500)]
    );
    assert!(
        result
            .prompt_text
            .contains("Never ask the player what genre"),
        "Genre section must include the fourth-wall guardrail instruction"
    );
}

#[test]
fn narrator_prompt_genre_identity_on_delta_tier() {
    // Genre identity must be injected on EVERY tier, not just Full.
    // On Delta tier, the narrator still needs to know what genre it's in.
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    let ctx = TurnContext {
        genre: Some("spaghetti_western".to_string()),
        ..Default::default()
    };

    let result =
        orch.build_narrator_prompt_tiered("draw my pistol", &ctx, NarratorPromptTier::Delta);

    assert!(
        result.prompt_text.contains("<genre>"),
        "Genre section must appear on Delta tier too"
    );
    assert!(
        result.prompt_text.contains("spaghetti western"),
        "Genre display name must be present on Delta tier"
    );
}

#[test]
fn narrator_prompt_no_genre_section_when_none() {
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    let ctx = TurnContext::default();
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        !result.prompt_text.contains("<genre>"),
        "No <genre> section when genre is None"
    );
}

#[test]
fn narrator_prompt_genre_in_primacy_zone() {
    // Genre identity must be in Primacy zone for maximum attention.
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);

    let ctx = TurnContext {
        genre: Some("mutant_wasteland".to_string()),
        ..Default::default()
    };

    let result = orch.build_narrator_prompt("look around", &ctx);

    // The genre section should appear before Valley-zone sections like game_state.
    // Since we're in Primacy, it should be near the top of the prompt.
    let genre_pos = result
        .prompt_text
        .find("<genre>")
        .expect("genre section must exist");
    // If there's a game_state section, genre must come before it
    if let Some(state_pos) = result.prompt_text.find("<game_state>") {
        assert!(
            genre_pos < state_pos,
            "Genre (Primacy) must appear before game_state (Valley) in prompt"
        );
    }
}
